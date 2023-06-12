use anyhow::Context;
use std::collections::HashMap;

#[cfg(feature = "prometheus")]
use std::net::SocketAddr;

use futures::stream::FuturesUnordered;
use ruuvi_sensor_protocol::SensorValues;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use tracing::{debug, error, info, warn};

use mybus::start_discovery;
use mybus::stop_discovery;

mod addr;
mod bluez;

#[cfg(feature = "modio")]
mod modio;
#[cfg(feature = "prometheus")]
mod prom;
#[cfg(feature = "prometheus")]
mod serve;

#[tracing::instrument(level = "trace")]
fn from_manuf(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<SensorValues> {
    for (k, v) in manufacturer_data {
        if let Ok(sensor) = SensorValues::from_manufacturer_specific_data(k, &v) {
            debug!(data = ?sensor, "Ruuvi tag data decoded");
            return Some(sensor);
        }
    }
    None
}

mod data {
    use crate::addr;

    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tracing::info;

    // Traits
    use ruuvi_sensor_protocol::{
        Acceleration, BatteryPotential, Humidity, MacAddress, MovementCounter, Pressure,
        Temperature, TransmitterPower,
    };

    #[tracing::instrument(level = "debug")]
    pub fn log_sensor(sensor: SensorValues) {
        let decoder = DecodedSensor::new(&sensor);
        if let Some(mac) = decoder.mac() {
            for (name, val, unit) in decoder {
                info!(
                    value = val,
                    name = name,
                    unit = unit,
                    mac = mac,
                    "Decoded value"
                );
            }
        }
    }

    pub async fn run_drain_channel(
        mut rx: mpsc::Receiver<SensorValues>,
        callback: fn(SensorValues) -> (),
    ) -> anyhow::Result<()> {
        use tokio::time::timeout;
        let mut count = 0;
        let delay = std::time::Duration::from_secs(120);

        info!("Prepared to discard remaining data");
        loop {
            if let Ok(val) = timeout(delay, rx.recv()).await {
                if let Some(sensor) = val {
                    count += 1;
                    callback(sensor);
                } else {
                    tracing::warn!(
                        discarded_messages = count,
                        "Upstream metadata collector hung up"
                    );
                    let err = anyhow::Error::msg("Upstream metadata collector hung up");
                    return Err(err);
                }
            } else {
                tracing::warn!(discarded_messages = count, period = ?delay, "No more data from the bus.");
                let err = anyhow::Error::msg("Timeout waiting for new data.");
                return Err(err);
            }
        }
    }

    #[derive(Debug)]
    struct DecodeError;

    // Enum for the state of the decoder. Is used as a state machine, as well as to track the
    // meaning of value and their units.
    #[derive(PartialEq)]
    enum DecidedSensorState {
        Humidity,
        Pressure,
        Temperature,
        Volts,
        Txpow,
        Movement,
        AccelX,
        AccelY,
        AccelZ,
        Done,
    }

    impl DecidedSensorState {
        pub fn as_str(&self) -> &str {
            use DecidedSensorState::*;
            match self {
                Humidity => "humidity",
                Pressure => "pressure",
                Temperature => "temperature",
                Volts => "volts",
                Txpow => "txpow",
                Movement => "movement",
                AccelX => "accel_x",
                AccelY => "accel_y",
                AccelZ => "accel_z",
                Done => "__done",
            }
        }

        // Tracking the unit here vs. in the decoder is a bit ugly, and requires them to be kept
        // in sync.
        pub fn senml_str(&self) -> &str {
            use DecidedSensorState::*;
            match self {
                Humidity => "/",
                Pressure => "Pa",
                Temperature => "Cel",
                Volts => "V",
                Txpow => "dBm",
                Movement => "count",
                AccelX => "g",
                AccelY => "g",
                AccelZ => "g",
                Done => "",
            }
        }
    }

    pub struct DecodedSensor<'sensor> {
        state: DecidedSensorState,
        sensor: &'sensor SensorValues,
    }

    impl DecodedSensor<'_> {
        pub fn new(sensor: &SensorValues) -> DecodedSensor<'_> {
            DecodedSensor {
                sensor,
                state: DecidedSensorState::Humidity,
            }
        }

        pub fn mac(&self) -> Option<String> {
            self.sensor
                .mac_address()
                .map(addr::Address::from)
                .map(|addr| addr.to_string())
                .map(|s| s.replace(':', ""))
        }

        /// Decode the current value (marked by internal state variable) as an Option<f64>
        fn decode(&self) -> Option<f64> {
            match self.state {
                DecidedSensorState::Humidity => self
                    .sensor
                    .humidity_as_ppm()
                    .map(f64::from)
                    .map(|num| num / 10000.0),
                DecidedSensorState::Pressure => self.sensor.pressure_as_pascals().map(f64::from),
                // .map(|num| num / 1000.0),
                DecidedSensorState::Temperature => self
                    .sensor
                    .temperature_as_millicelsius()
                    .map(f64::from)
                    .map(|num| num / 1000.0),
                DecidedSensorState::Volts => self
                    .sensor
                    .battery_potential_as_millivolts()
                    .map(f64::from)
                    .map(|num| num / 1000.0),
                DecidedSensorState::Txpow => self.sensor.tx_power_as_dbm().map(f64::from),
                DecidedSensorState::Movement => self.sensor.movement_counter().map(f64::from),
                DecidedSensorState::AccelX => self
                    .sensor
                    .acceleration_vector_as_milli_g()
                    .map(|vec| f64::from(vec.0))
                    .map(|num| num / 1000.0),
                DecidedSensorState::AccelY => self
                    .sensor
                    .acceleration_vector_as_milli_g()
                    .map(|vec| f64::from(vec.1))
                    .map(|num| num / 1000.0),
                DecidedSensorState::AccelZ => self
                    .sensor
                    .acceleration_vector_as_milli_g()
                    .map(|vec| f64::from(vec.2))
                    .map(|num| num / 1000.0),
                DecidedSensorState::Done => None,
            }
        }
    }

    impl Iterator for DecodedSensor<'_> {
        type Item = (String, String, String);
        fn next(&mut self) -> Option<Self::Item> {
            if self.state == DecidedSensorState::Done {
                return None;
            }
            let step = self.state.as_str().to_string();
            let unit = self.state.senml_str().to_string();
            let val = self.decode();
            self.state = match self.state {
                DecidedSensorState::Humidity => DecidedSensorState::Pressure,
                DecidedSensorState::Pressure => DecidedSensorState::Temperature,
                DecidedSensorState::Temperature => DecidedSensorState::Volts,
                DecidedSensorState::Volts => DecidedSensorState::Txpow,
                DecidedSensorState::Txpow => DecidedSensorState::Movement,
                DecidedSensorState::Movement => DecidedSensorState::AccelX,
                DecidedSensorState::AccelX => DecidedSensorState::AccelY,
                DecidedSensorState::AccelY => DecidedSensorState::AccelZ,
                DecidedSensorState::AccelZ => DecidedSensorState::Done,
                DecidedSensorState::Done => DecidedSensorState::Done,
            };
            val.map(|val| (step, val.to_string(), unit))
        }
    }
}

mod mybus {
    use std::collections::HashMap;

    use std::fmt;

    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;
    use tracing::{debug, info, trace, warn};
    use zbus::zvariant::OwnedObjectPath;
    use zbus::ProxyDefault; // Trait

    use crate::bluez::adapter1::Adapter1Proxy;
    use crate::bluez::device1::Device1Proxy;
    use crate::from_manuf;

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn find_adapters(
        connection: &zbus::Connection,
    ) -> anyhow::Result<Vec<Adapter1Proxy>> {
        let mut result: Vec<Adapter1Proxy> = Vec::new();

        let p = zbus::fdo::ObjectManagerProxy::builder(connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let managed = p.get_managed_objects().await?;

        for (path, children) in &managed {
            for interface in children.keys() {
                // for interface, _props in children {
                // This will print the found paths, devices and their metadata
                // println!("path={:?} interface={} muh={:?}", &path, interface, _props);
                if interface.as_str() == Adapter1Proxy::INTERFACE {
                    let adapter = Adapter1Proxy::builder(connection)
                        .destination("org.bluez")?
                        .path(path.clone())?
                        .build()
                        .await?;
                    result.push(adapter);
                }
            }
        }
        Ok(result)
    }

    #[derive(Debug)]
    pub struct MyDev<'device> {
        device: Device1Proxy<'device>,
        listener: tokio::task::JoinHandle<anyhow::Result<()>>,
        name: Option<String>,
        address: Option<String>,
    }
    impl MyDev<'static> {
        #[tracing::instrument(name = "MyDev::new", skip(device, tx), fields(name, address))]
        pub async fn new(
            device: Device1Proxy<'static>,
            tx: mpsc::Sender<SensorValues>,
        ) -> anyhow::Result<MyDev<'static>> {
            use anyhow::Context;
            let address = device.address().await.ok();
            let name = device.name().await.ok();
            if name.is_some() {
                tracing::Span::current().record("name", &name);
            };
            if address.is_some() {
                tracing::Span::current().record("address", &address);
            }

            debug!(message = "Gathering manufacturer data");
            if let Ok(manuf_data) = device.manufacturer_data().await {
                if let Some(sens) = from_manuf(manuf_data) {
                    tx.send(sens)
                        .await
                        .with_context(|| "Sensor processor hung up")?;
                }
            };
            let stream = device.receive_manufacturer_data_changed().await;
            // Spawn a task to poll this device's stream
            let listener = tokio::spawn(manufacturer_listener(stream, tx));

            let res = Self {
                device,
                listener,
                name,
                address,
            };
            Ok(res)
        }

        pub async fn from_data(
            connection: &zbus::Connection,
            object_path: OwnedObjectPath,
            tx: mpsc::Sender<SensorValues>,
        ) -> anyhow::Result<MyDev<'static>> {
            let dev_proxy = Device1Proxy::builder(connection)
                .destination("org.bluez")?
                .path(object_path)?
                .cache_properties(zbus::CacheProperties::Yes)
                .build()
                .await?;
            MyDev::new(dev_proxy, tx).await
        }

        // Tell the tracing infra to use Display formating of "self" as the "device" field.
        #[tracing::instrument(skip(self), fields(device = %self))]
        pub async fn bye(self) {
            let _ = &self.listener.abort();
            if (self.listener.await).is_ok() {
                warn!(message = "Unexpectedly task succeeded before being aborted");
            }
            drop(self.device);
        }
    }

    impl<'device> fmt::Display for MyDev<'device> {
        // This trait requires `fmt` with this exact signature.
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            // Write strictly the first element into the supplied output
            // stream: `f`. Returns `fmt::Result` which indicates whether the
            // operation succeeded or failed. Note that `write!` uses syntax which
            // is very similar to `println!`.

            let path = self.device.path();
            write!(f, "MyDev(")?;
            match self.name.as_ref() {
                Some(name) => write!(f, "name={}, ", &name)?,
                None => write!(f, "name=, ")?,
            }

            match self.address.as_ref() {
                Some(addr) => write!(f, "address={}, ", &addr)?,
                None => write!(f, "address=, ")?,
            }
            write!(f, "dbus_path={})", path)
        }
    }

    #[tracing::instrument(skip_all, fields(name))]
    pub async fn adapter_start_discovery(adapter: &Adapter1Proxy<'_>) -> anyhow::Result<()> {
        let name = adapter.name().await?;
        tracing::Span::current().record("name", &name);

        info!(message = "Powering adapter on");
        adapter.set_powered(true).await?;
        // discovery filter is a map str=>val with fixed keys
        // see
        //  https://github.com/bluez-rs/bluez-async/blob/main/bluez-async/src/lib.rs#L143
        // and
        //  https://github.com/Vudentz/BlueZ/blob/master/doc/adapter-api.txt#L49 for docs
        //
        adapter.set_discovery_filter(HashMap::new()).await?;
        info!(message = "Starting adapter discovery");
        adapter.start_discovery().await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn stop_discovery(connection: &zbus::Connection) -> anyhow::Result<()> {
        for adapter in find_adapters(connection).await? {
            let name = adapter.name().await?;
            info!(name = name, "Stopping adapter discovery");
            adapter.stop_discovery().await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn start_discovery(connection: &zbus::Connection) -> anyhow::Result<()> {
        let adapters = find_adapters(connection).await?;
        for adapter in &adapters {
            adapter_start_discovery(adapter).await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all, fields(name))]
    async fn manufacturer_data_one(
        v: zbus::PropertyChanged<'_, HashMap<u16, Vec<u8>>>,
    ) -> anyhow::Result<Option<SensorValues>> {
        // Mostly internal cruft that could live in "manufacturer_listener" but turns the inner
        // logic hard to read, and also ends up doing somewhat nasty things to the current span
        // with fields.
        // Thats why it's an helper
        let name = v.name();
        tracing::Span::current().record("name", v.name());
        let value = v.get().await;
        if name == "ManufacturerData" {
            trace!(message = "Device Signal",  value = ?value);
            if let Ok(val) = value {
                return Ok(from_manuf(val));
            }
        } else {
            warn!(message = "Unknown signal", value = ?value);
        }
        Ok(None)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn manufacturer_listener(
        mut stream: zbus::PropertyStream<'_, HashMap<u16, Vec<u8>>>,
        tx: mpsc::Sender<SensorValues>,
    ) -> anyhow::Result<()> {
        while let Some(v) = stream.next().await {
            if let Some(decoded) = manufacturer_data_one(v).await? {
                tx.send(decoded).await?
            }
        }
        Ok(())
    }
}
mod devices {
    use std::collections::HashMap;
    use std::convert::From;

    use std::sync::{Arc, Mutex};

    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;
    use tracing::{error, info};
    use zbus::zvariant::OwnedObjectPath;
    use zbus::ProxyDefault; // Trait

    use crate::bluez::adapter1::Adapter1Proxy;
    use crate::bluez::device1::Device1Proxy;

    use crate::mybus::start_discovery;
    use crate::mybus::MyDev;
    type DeviceHash<'device> = Arc<Mutex<HashMap<OwnedObjectPath, MyDev<'device>>>>;

    #[derive(Debug)]
    pub struct LostFound {
        tx: mpsc::Sender<SensorValues>,
        connection: zbus::Connection,
        pmap: DeviceHash<'static>,
    }

    impl LostFound {
        pub fn new(tx: mpsc::Sender<SensorValues>, connection: zbus::Connection) -> LostFound {
            // Mapping of  the device path => device
            // Used so we can remove device proxies that are out of date.
            let pmap = Arc::new(Mutex::new(HashMap::<OwnedObjectPath, MyDev>::new()));
            LostFound {
                tx,
                connection,
                pmap,
            }
        }

        #[tracing::instrument(level = "info", skip_all)]
        pub async fn handle_removed_signal(
            &self,
            data: zbus::fdo::InterfacesRemovedArgs<'_>,
        ) -> anyhow::Result<()> {
            let object_path = OwnedObjectPath::from(data.object_path);
            // Having the debug line below forces us to de-serialize the entire rest even when
            // we don't care about it.
            // debug!(message = "DBus Interface removed", data = ?data);

            // Since the mutex guard cant be held across await, we take it in a closure and
            // return the resulting one, before going in.
            // That way we don't hold the mutex across thread jumps
            let device = {
                let mut devices = match self.pmap.lock() {
                    Ok(devices) => devices,
                    Err(err) => {
                        error!(err = ? err, message = "Another worker died while holding a mutex lock");
                        return Err(anyhow::Error::msg(
                            "Another worker died while holding a mutex lock",
                        ));
                    }
                };
                devices.remove(&object_path)
            };
            if let Some(device) = device {
                info!(message = "Device disconnected", device = %device);
                device.bye().await;
            }
            Ok(())
        }

        #[tracing::instrument(level = "info", skip_all)]
        async fn store_device(
            &self,
            object_path: OwnedObjectPath,
            device: MyDev<'static>,
        ) -> anyhow::Result<()> {
            {
                let old = {
                    let mut devices = match self.pmap.lock() {
                        Ok(devices) => devices,
                        Err(err) => {
                            error!(err = ? err, message = "Failed to unlock due to poison");
                            return Err(anyhow::Error::msg(
                                "Another worker died while holding a mutex lock",
                            ));
                        }
                    };
                    devices.insert(object_path, device)
                };
                if let Some(old) = old {
                    info!(message = "Dropping old device for path", old = ?old);
                    old.bye().await;
                }
            }
            Ok(())
        }
        #[tracing::instrument(level = "info", skip_all)]
        pub async fn handle_added_signal(
            &self,
            data: zbus::fdo::InterfacesAddedArgs<'_>,
        ) -> anyhow::Result<()> {
            use anyhow::Context;
            // Data looks like this:
            // Interfaces Added: data=InterfacesAdded { object_path: ObjectPath("/org/bluez/hci0/dev_C4_47_33_92_F2_96"), interfaces_and_properties: {"org.freedesktop.DBus.Introspectable": {}, "org.bluez.Device1": {"Alias": Str(Str(Borrowed("C4-47-33-92-F2-96"))), "Trusted": Bool(false), "Connected": Bool(false), "Adapter": ObjectPath(ObjectPath("/org/bluez/hci0")), "UUIDs": Array(Array { element_signature: Signature("s"), elements: [], signature: Signature("as") }), "Paired": Bool(false), "AddressType": Str(Str(Borrowed("random"))), "Blocked": Bool(false), "ServicesResolved": Bool(false), "Address": Str(Str(Borrowed("C4:47:33:92:F2:96"))), "Bonded": Bool(false), "AdvertisingFlags": Array(Array { element_signature: Signature("y"), elements: [U8(0)], signature: Signature("ay") }), "LegacyPairing": Bool(false), "ManufacturerData": Dict(Dict { entries: [DictEntry { key: U16(76), value: Value(Array(Array { element_signature: Signature("y"), elements: [U8(18), U8(2), U8(0), U8(2)], signature: Signature("ay") })) }], key_signature: Signature("q"), value_signature: Signature("v"), signature: Signature("a{qv}") }), "RSSI": I16(-77)}, "org.freedesktop.DBus.Properties": {}} }

            // Clone connection to avoid lifetime issues
            let connection = self.connection.clone();
            let tx = self.tx.clone();
            // We force a new discovery if it's an adapter.
            if data
                .interfaces_and_properties
                .iter()
                .any(|(interface, _)| interface == &Adapter1Proxy::INTERFACE)
            {
                start_discovery(&connection).await.with_context(|| {
                    error!(message = "Failed to start discovery on new adapter", data=?data);
                    format!("Failed to start discovery on new adapter {data:?}")
                })?;
            }
            let object_path = OwnedObjectPath::from(data.object_path);
            if data
                .interfaces_and_properties
                .iter()
                .any(|(interface, _)| interface == &Device1Proxy::INTERFACE)
            {
                // Clone the data so we can just toss it without caring about lifetimes.
                // The "biggest" is a string for the path.
                let device = MyDev::from_data(&connection, object_path.clone(), tx.clone()).await?;
                self.store_device(object_path, device).await?;
            }
            Ok(())
        }

        #[tracing::instrument(level = "info", skip_all)]
        pub async fn initial_subscription(&self) -> anyhow::Result<()> {
            let connection = self.connection.clone();
            info!(message = "Subscribing to pre-existing devices");
            for dev_proxy in find_devices(&connection).await? {
                let object_path = OwnedObjectPath::from(dev_proxy.path().to_owned());
                let device = MyDev::new(dev_proxy, self.tx.clone()).await?;
                self.store_device(object_path, device).await?;
            }
            Ok(())
        }
    }
    /// Get a list of all Bluetooth devices which have been discovered so far.
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn find_devices<'device>(
        connection: &zbus::Connection,
    ) -> anyhow::Result<Vec<Device1Proxy<'device>>> {
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let managed = bluez_root.get_managed_objects().await?;

        // Filter down to only the pairs that match our interface
        let object_paths: Vec<OwnedObjectPath> = managed
            .into_iter()
            .filter_map(|(object_path, mut children)| {
                // Children is a hashmap<Interface, Data>
                children
                    .remove(Device1Proxy::INTERFACE)
                    // data is HashMap<String,Value>
                    .map(|data| (object_path, data))
            })
            .map(|(object_path, _)| object_path)
            .collect();

        let mut result: Vec<Device1Proxy> = Vec::new();
        for object_path in object_paths {
            let device = Device1Proxy::builder(connection)
                .destination("org.bluez")?
                .path(object_path)?
                .cache_properties(zbus::CacheProperties::Yes)
                .build()
                .await?;
            result.push(device);
        }
        // The above is cumbersomeely visiting all data since I wanted to debug it, and then throws
        // it away in the last map.  That should be fine as we don't do this often, only at start.
        Ok(result)
    }

    use zbus::fdo::InterfacesAddedStream;
    use zbus::fdo::InterfacesRemovedStream;

    // Inner function so we can get cleaner error handling.
    async fn subscribe_interfaces(
        connection: &zbus::Connection,
    ) -> anyhow::Result<(
        InterfacesRemovedStream<'static>,
        InterfacesAddedStream<'static>,
    )> {
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let removed = bluez_root.receive_interfaces_removed().await?;
        let added = bluez_root.receive_interfaces_added().await?;
        Ok((removed, added))
    }

    pub async fn run_lostfound_actor(actor: LostFound) -> anyhow::Result<()> {
        use anyhow::Context;
        // Lets try to get some changes on the devices
        let (mut removed, mut added) = subscribe_interfaces(&actor.connection)
            .await
            .with_context(|| "Failed to subscribe to interface changes")?;

        // Only iterate and subscribe _after_ we have set up the listeners, or we may (maybe) miss
        // a device that appeared in this tiny race window.
        actor
            .initial_subscription()
            .await
            .with_context(|| "Failed during intiial subscription to Bluetooth devices.")?;
        info!("Tracking bluetooth device appearance and disappearance");
        loop {
            tokio::select! {
                Some(rem_sig) = removed.next() => {
                    if let Ok(data) = rem_sig.args() {
                        actor.handle_removed_signal(data).await?
                    }
                },
                Some(add_sig) = added.next() => {
                    if let Ok(data) = add_sig.args() {
                        actor.handle_added_signal(data).await?
                    }
                },
                else => break,
            }
        }
        Err(anyhow::Error::msg(
            "lostfound dbus handler failed succesfully",
        ))
    }
}

async fn real_main() -> anyhow::Result<()> {
    let mut connection = zbus::Connection::system().await?;
    connection.set_max_queued(1200);

    let (new_devices_tx, rx) = mpsc::channel(100);

    let lostfound_actor = devices::LostFound::new(new_devices_tx, connection.clone());

    info!(message = "Setting up interface add and remove signals");

    let mut tasks = vec![
        // Sensor data processor
        tokio::spawn(devices::run_lostfound_actor(lostfound_actor)),
    ];

    // If we enable the feature, we add another pair of channels, passing the receiver of the
    // previous to this, and the new TX into it, so messages are worked on and passed on.
    #[cfg(feature = "modio")]
    let rx = if cfg!(feature = "modio") {
        let (modio_tx, modio_rx) = mpsc::channel(100);
        let modio_actor = modio::SensorActor::new(rx, modio_tx).await?;
        tasks.push(tokio::spawn(modio::run_sensor_actor(modio_actor)));
        modio_rx
    } else {
        rx
    };

    // If prometheus is enabled, we also run the web-server
    // Otherwise, we do not...

    #[cfg(feature = "prometheus")]
    if cfg!(feature = "prometheus") {
        // Web server
        let address: SocketAddr = "0.0.0.0:9185".parse()?;
        tasks.push(tokio::spawn(serve::webserver(address)));
    };

    #[cfg(feature = "prometheus")]
    let rx = if cfg!(feature = "prometheus") {
        let (prom_tx, prom_rx) = mpsc::channel(100);
        let prom_actor = prom::SensorActor::new(rx, prom_tx);
        tasks.push(tokio::spawn(prom::run_sensor_actor(prom_actor)));
        prom_rx
    } else {
        rx
    };

    // Last in the chain we drain values and discard them.
    if cfg!(feature = "modio") || cfg!(feature = "prometheus") {
        // Drain the pipe at the end, leaving only silence.
        tasks.push(tokio::spawn(data::run_drain_channel(rx, |_| {})));
    }
    // But if we have neither modio nor prometheus sinks, we log them as we drain.
    else {
        tasks.push(tokio::spawn(data::run_drain_channel(rx, data::log_sensor)));
    }

    start_discovery(&connection)
        .await
        .with_context(|| "Failed to start discovery. Airplane mode?")?;

    let mut futs = tasks
        .into_iter()
        .collect::<FuturesUnordered<JoinHandle<_>>>();

    let delay = std::time::Duration::from_secs(30);
    let mut waiter = tokio::time::interval(delay);

    // Since our tasks failing will cause a cascade, but some tasks may still be running,  timeouts may happen, it's not always that
    // the first error seen is the one causing an issue.
    // Thus, we iterate for all of them, and log all the errors.
    let mut first_error = None;

    // This is a bit more complex than I want it to be, but as we do not want to see the FIRST
    // error that happens, as that may not be the _root_cause_ error. Fex, modio-logger
    // disconnecting may cause the next one in line to drop, as it will get "writer to my read
    // channel hung up" error.
    //
    // Thus, I've implemented some hacky loop here. We loop forever, but in intervals of $delay, we
    // sleep, but break if there _is_ an
    // error and we have looped for a certain time.
    //
    // In the first branch, however, we check the results and attempt to log tasks that succeed
    // unexpectedly, and tasks that fail
    //
    // Succeed unexpectedly will happen if we wait for events and get notified that the write
    // side disappeared, this means it was a victim of another process terminating, which is Ok,
    // but an error
    //
    loop {
        tokio::select! {
            task = futs.next() => {
                // Futs.next() is a future,  None means no more tasks, Some means task.
                if let Some(task) = task {
                    let task_result = task?;
                    match task_result {
                        Ok(_) => warn!(message = "Task ended succesfully but shouldn't have"),
                        Err(err) => {
                            error!(err = ?err, errtxt = err.to_string(), "Task ended badly.");
                            // Replace content in `first_error` if it is None
                            first_error.get_or_insert(err);
                        }
                    }
                // None, we have no more tasks. Not breaking here would cause an infinte loop.
                } else {
                    warn!("Consumed the last worker task. Exiting.");
                    break;
                }
            }
            _t = waiter.tick() => {
                if first_error.is_some() {
                    warn!("Timeout happened and we have an error. Exiting.");
                    break;
                }
            }
        }
    }
    stop_discovery(&connection).await?;
    if let Some(err) = first_error {
        return Err(err);
    };
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    real_main().await
}
