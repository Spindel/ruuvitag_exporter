use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;

use futures::stream::FuturesUnordered;
use ruuvi_sensor_protocol::SensorValues;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use tracing::{error, info, warn};

use mybus::find_adapters;
use mybus::start_discovery;

mod addr;
mod bluez;
mod prom;
mod serve;

fn from_manuf(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<SensorValues> {
    for (k, v) in manufacturer_data {
        if let Ok(sensor) = SensorValues::from_manufacturer_specific_data(k, &v) {
            return Some(sensor);
        }
    }
    None
}

mod modio {
    use crate::addr;

    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tracing::info;

    pub struct SensorActor {
        receiver: mpsc::Receiver<SensorValues>,
        sender: mpsc::Sender<SensorValues>,
        connection: zbus::Connection,
    }
    impl SensorActor {
        pub fn new(
            receiver: mpsc::Receiver<SensorValues>,
            sender: mpsc::Sender<SensorValues>,
            connection: zbus::Connection,
        ) -> Self {
            SensorActor {
                receiver,
                sender,
                connection,
            }
        }

        pub async fn handle_message(
            &mut self,
            msg: SensorValues,
        ) -> Result<(), tokio::sync::mpsc::error::SendError<SensorValues>> {
            modio_log_sensor(&self.connection, &msg).await;
            self.sender.send(msg).await
        }
    }

    pub async fn run_sensor_actor(mut actor: SensorActor) {
        while let Some(msg) = actor.receiver.recv().await {
            actor
                .handle_message(msg)
                .await
                .expect("Unknown failure in channel");
        }
    }

    #[derive(Debug)]
    struct DecodeError;
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

    struct DecodedSensor<'sensor> {
        state: DecidedSensorState,
        sensor: &'sensor SensorValues,
    }
    // Traits
    use ruuvi_sensor_protocol::{
        Acceleration, BatteryPotential, Humidity, MacAddress, MovementCounter, Pressure,
        Temperature, TransmitterPower,
    };

    impl DecodedSensor<'_> {
        fn new<'sensor>(sensor: &'sensor SensorValues) -> DecodedSensor<'sensor> {
            DecodedSensor {
                sensor: sensor,
                state: DecidedSensorState::Humidity,
            }
        }

        fn mac(&self) -> Option<String> {
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

    #[tracing::instrument]
    async fn modio_log_sensor(connection: &zbus::Connection, sensor: &SensorValues) {
        let ipc = fsipc::legacy::fsipcProxy::builder(&connection)
            .build()
            .await
            .unwrap();
        let decoder = DecodedSensor::new(&sensor);
        if let Some(mac) = decoder.mac() {
            for (name, val, unit) in decoder {
                info!(value = val, name = name, unit = unit, "have a decoded v");
                let key = format!("ruuvi.{mac}.{name}");
                ipc.store(&key, &val).await.unwrap();
            }
        }
    }
}

mod mybus {
    use std::collections::HashMap;

    use std::fmt;
    use std::sync::{Arc, Mutex};

    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;
    use tracing::{debug, error, info, trace, warn};
    use zbus::zvariant::OwnedObjectPath;
    use zbus::ProxyDefault; // Trait

    use crate::bluez::adapter1::Adapter1Proxy;
    use crate::bluez::device1::Device1Proxy;
    use crate::from_manuf;
    type DeviceHash<'device> = Arc<Mutex<HashMap<OwnedObjectPath, MyDev<'device>>>>;

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn find_adapters(connection: &zbus::Connection) -> zbus::Result<Vec<Adapter1Proxy>> {
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
        listener: tokio::task::JoinHandle<()>,
        name: Option<String>,
        address: Option<String>,
    }
    impl MyDev<'static> {
        #[tracing::instrument(name = "MyDev::new", skip(device, tx), fields(name, address))]
        pub async fn new(
            device: Device1Proxy<'static>,
            tx: mpsc::Sender<SensorValues>,
        ) -> zbus::Result<MyDev<'static>> {
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
                    if let Err(err) = tx.send(sens).await {
                        error!(message = "Sensor listener hung up?", err= ?err);
                    }
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
        ) -> zbus::Result<MyDev<'static>> {
            let dev_proxy = Device1Proxy::builder(&connection)
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
                None => write!(f, "no_name, ")?,
            }

            match self.address.as_ref() {
                Some(addr) => write!(f, "address={}, ", &addr)?,
                None => write!(f, "no_address, ")?,
            }
            write!(f, "dbus_path={})", path)
        }
    }

    #[tracing::instrument(skip_all, fields(name))]
    pub async fn adapter_start_discovery(adapter: &Adapter1Proxy<'_>) -> zbus::Result<()> {
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
    pub async fn start_discovery(connection: &zbus::Connection) -> zbus::Result<()> {
        let adapters = find_adapters(connection).await?;
        for adapter in &adapters {
            adapter_start_discovery(adapter).await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn manufacturer_listener(
        mut stream: zbus::PropertyStream<'_, HashMap<u16, Vec<u8>>>,
        tx: mpsc::Sender<SensorValues>,
    ) {
        while let Some(v) = stream.next().await {
            let value = v.get().await;
            trace!(message = "Device Signal",  name = ?v.name(),  value = ?value);
            if v.name() == "ManufacturerData" {
                if let Ok(val) = v.get().await {
                    if let Some(sens) = from_manuf(val) {
                        debug!(message = "Ruuvi data decoded",  data = ?sens);
                        if let Err(err) = tx.send(sens).await {
                            error!(message = "Failed to process sensor value", err = ?err);
                        }
                    }
                }
            } else {
                warn!(message = "Unknown signal", name = ?v.name(), value = ?value);
            }
        }
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
        pub async fn handle_removed_signal(&self, data: zbus::fdo::InterfacesRemovedArgs<'_>) {
            let object_path = OwnedObjectPath::from(data.object_path);
            // Having the debug line below forces us to de-serialize the entire rest even when
            // we don't care about it.
            // debug!(message = "DBus Interface removed", data = ?data);

            // Since the mutex guard cant be held across await, we take it in a closure and
            // return the resulting one, before going in.
            // That way we don't hold the mutex across thread jumps
            let device = {
                let mut devices = self.pmap.lock().expect("Failed to lock devices");
                devices.remove(&object_path)
            };
            if let Some(device) = device {
                info!(message = "Device disconnected", device = %device);
                device.bye().await;
            }
        }

        #[tracing::instrument(level = "info")]
        async fn insert_device(&self, object_path: OwnedObjectPath, device: MyDev<'static>) {
            {
                let old = {
                    let mut devices = match self.pmap.lock() {
                        Ok(devices) => devices,
                        Err(err) => {
                            // TODO: Figure out error handling
                            error!(message = "Failed to unlock", err=?err);
                            return;
                        }
                    };
                    devices.insert(object_path, device)
                };
                if let Some(old) = old {
                    info!(message = "Dropping old device for path", old = ?old);
                    old.bye().await;
                }
            }
        }
        #[tracing::instrument(level = "info", skip_all)]
        pub async fn handle_added_signal(&self, data: zbus::fdo::InterfacesAddedArgs<'_>) {
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
                if let Err(err) = start_discovery(&connection).await {
                    error!(message = "Failed to start discovery on new adapter",  err=?err, data=?data);
                }
            }
            let object_path = OwnedObjectPath::from(data.object_path);
            if data
                .interfaces_and_properties
                .iter()
                .any(|(interface, _)| interface == &Device1Proxy::INTERFACE)
            {
                // Clone the data so we can just toss it without caring about lifetimes.
                // The "biggest" is a string for the path.
                if let Ok(device) =
                    MyDev::from_data(&connection, object_path.clone(), tx.clone()).await
                {
                    self.insert_device(object_path, device).await;
                }
            }
        }

        pub async fn initial_subscription(&self) -> Result<(), Box<dyn std::error::Error>> {
            let connection = self.connection.clone();

            for dev_proxy in find_devices(&connection).await? {
                let object_path = OwnedObjectPath::from(dev_proxy.path().to_owned());
                let device = MyDev::new(dev_proxy, self.tx.clone()).await?;
                self.insert_device(object_path, device).await;
            }
            Ok(())
        }
    }
    /// Get a list of all Bluetooth devices which have been discovered so far.
    #[tracing::instrument(level = "info", skip_all)]
    pub async fn find_devices<'device>(
        connection: &zbus::Connection,
    ) -> zbus::Result<Vec<Device1Proxy<'device>>> {
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

    pub async fn run_lostfound_actor(actor: LostFound) {
        // Lets try to get some changes on the devices
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(&actor.connection)
            .destination("org.bluez")
            .unwrap()
            .path("/")
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut removed = bluez_root.receive_interfaces_removed().await.unwrap();
        let mut added = bluez_root.receive_interfaces_added().await.unwrap();
        // Only iterate and subscribe _after_ we have set up the listeners, or we may (maybe) miss
        // a device that appeared in this tiny race window.
        actor
            .initial_subscription()
            .await
            .expect("Failed to subscribe to devices");

        loop {
            tokio::select! {
                Some(rem_sig) = removed.next() => {
                    if let Ok(data) = rem_sig.args() {
                        actor.handle_removed_signal(data).await
                    }
                },
                Some(add_sig) = added.next() => {
                    if let Ok(data) = add_sig.args() {
                        actor.handle_added_signal(data).await
                    }
                },
                else => break,
            }
        }
    }
}

async fn real_main() -> Result<(), Box<dyn Error>> {
    let mut connection = zbus::Connection::system().await?;
    connection.set_max_queued(1200);

    let (tx, rx) = mpsc::channel(100);
    let (tx2, rx2) = mpsc::channel(100);
    let (tx3, _rx3) = mpsc::channel(100);

    info!(message = "Setting up interface add and remove signals");

    let address: SocketAddr = "0.0.0.0:9185".parse()?;

    // modio connection stuff
    let session = std::env::args().any(|arg| arg == "--session");
    let modio_connection = if session {
        zbus::Connection::session().await.unwrap()
    } else {
        zbus::Connection::system().await.unwrap()
    };

    let prom_actor = prom::SensorActor::new(rx, tx2);
    let modio_actor = modio::SensorActor::new(rx2, tx3, modio_connection);
    let lostfound_actor = devices::LostFound::new(tx.clone(), connection.clone());
    let tasks = [
        // Web server
        tokio::spawn(serve::webserver(address)),
        // Sensor data processor
        tokio::spawn(prom::run_sensor_actor(prom_actor)),
        tokio::spawn(modio::run_sensor_actor(modio_actor)),
        tokio::spawn(devices::run_lostfound_actor(lostfound_actor)),
    ];

    if let Err(err) = start_discovery(&connection).await {
        error!(message = "Failed to start discovery. Airplane mode?", err = %err);
        return Err(Box::new(err));
    }

    let mut futs = tasks
        .into_iter()
        .collect::<FuturesUnordered<JoinHandle<_>>>();

    if let Some(task_result) = futs.next().await {
        error!("Something ended and I do not know what or why.");
        match task_result {
            Ok(_) => warn!(message = "Task ended succesfully"),
            Err(err) => error!(message = "Task ended badly.", err = ?err),
        }
    }
    // All done, shut down
    for adapter in find_adapters(&connection).await? {
        adapter.stop_discovery().await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    real_main().await
}
