#![forbid(unsafe_code)]
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures::stream::FuturesUnordered;
use ruuvi_sensor_protocol::SensorValues;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use tracing::{error, info, warn};
use zbus::zvariant::OwnedObjectPath;

use mybus::device_found;
use mybus::device_lost;
use mybus::find_adapters;
use mybus::find_devices;
use mybus::start_discovery;
use mybus::MyDev;

mod bluer;
mod bluez;

fn from_manuf(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<SensorValues> {
    for (k, v) in manufacturer_data {
        if let Ok(sensor) = SensorValues::from_manufacturer_specific_data(k, &v) {
            return Some(sensor);
        }
    }
    None
}

mod prom {
    #![forbid(unsafe_code)]
    use crate::bluer::Address;
    use lazy_static::lazy_static;
    use prometheus::{self, GaugeVec, IntCounterVec, IntGaugeVec};
    use prometheus::{
        labels, opts, register_gauge_vec, register_int_counter_vec, register_int_gauge_vec,
    };
    use ruuvi_sensor_protocol::SensorValues;
    use tokio::sync::mpsc;
    use tracing::{error, field, info, span, warn, Level};

    lazy_static! {
        static ref TEMPERATURE: GaugeVec =
            register_gauge_vec!(opts!("temperature", "Temperature in Celsius", labels!("unit" => "Cel")), &["mac"]).expect("Failed to initialize");

        static ref HUMIDITY: GaugeVec =
            register_gauge_vec!(opts!("humidity", "Humidity in percent", labels!("unit" => "%RH")), &["mac"]).expect("Failed to initialize");

        static ref PRESSURE: GaugeVec =
            register_gauge_vec!(opts!("pressure", "Pressure in pascals", labels!("unit" => "Pa")), &["mac"]).expect("Failed to initialize");

        static ref POTENTIAL: GaugeVec =
            register_gauge_vec!(opts!("battery_potential", "Battery potential in volts", labels!("unit" => "V")), &["mac"]).expect("Failed to initialize");

        static ref TXPOW: IntGaugeVec =
            register_int_gauge_vec!(opts!("tx_power", "BLE TX power as dBm", labels!("unit" => "dBm")), &["mac"]).expect("Failed to initialize");

        // It wraps too often to be a good counter.
        static ref MOVEMENT: IntGaugeVec =
            register_int_gauge_vec!(opts!("movement_counter", "Device movement counter"), &["mac"]).expect("Failed to initialize");

        static ref ACCEL: GaugeVec =
            register_gauge_vec!(opts!("acceleration", "Acceleration of the device"), &["mac", "axis"]).expect("Failed to initialize");

        static ref MEASURE_NO: IntGaugeVec =
            register_int_gauge_vec!(opts!("measurement_sequence_no", "The sequence number of ruuvi-emitted signals"), &["mac"]).expect("Failed to initialize");

        static ref COUNT: IntCounterVec =
            register_int_counter_vec!(opts!("signals", "The amount of processed ruuvi signals"), &["mac"]).expect("Failed to initialize");

    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) async fn sensor_processor(mut rx: mpsc::Receiver<SensorValues>) {
        const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
        // Shouldn't this be easier?
        // The flow here looks really convoluted in my opinion.
        loop {
            // If no data arrives within TIMEOUT,  abort the wait.
            match tokio::time::timeout(TIMEOUT, rx.recv()).await {
                Err(_) => {
                    // Timeout happened.
                    // Log the error and the timeout, then return from this task.
                    error!(message = "No new values", timeout = TIMEOUT.as_secs());
                    return;
                }
                Ok(rxval) => match rxval {
                    // some data arrived. All is good
                    Some(sensor) => register_sensor(&sensor),
                    // None happens when there are no senders left.
                    None => {
                        warn!("No data-emitter tasks left.");
                        return;
                    }
                },
            }
        }
    }

    fn register_sensor(sensor: &SensorValues) {
        // Traits
        use ruuvi_sensor_protocol::{
            Acceleration, BatteryPotential, Humidity, MacAddress, MeasurementSequenceNumber,
            MovementCounter, Pressure, Temperature, TransmitterPower,
        };
        // It is important that the keys in the span match what we use in `span.record("key",...)` below.
        let span = span!(
            Level::INFO,
            "register_data",
            mac = field::Empty,
            temperature = field::Empty,
            humidity = field::Empty,
            pressure = field::Empty,
            movement = field::Empty,
            volts = field::Empty,
            txpow = field::Empty,
            accel_x = field::Empty,
            accel_y = field::Empty,
            accel_z = field::Empty,
            measure_no = field::Empty
        );
        let enter = span.enter();

        let mac = if let Some(mac) = sensor.mac_address() {
            Address::from(mac).to_string()
        } else {
            warn!("Cannot process sensor: {sensor:?}", sensor = sensor);
            return;
        };
        // Tracing has a hard time with String, this wraps it as a borrowed value
        span.record("mac", &tracing::field::display(&mac));

        COUNT.with_label_values(&[&mac]).inc();
        if let Some(humidity) = sensor.humidity_as_ppm() {
            let humidity = f64::from(humidity) / 10000.0;
            span.record("humidity", humidity);
            HUMIDITY.with_label_values(&[&mac]).set(humidity);
        }

        if let Some(pressure) = sensor.pressure_as_pascals() {
            let pressure = f64::from(pressure) / 1000.0;
            span.record("pressure", pressure);
            PRESSURE.with_label_values(&[&mac]).set(pressure);
        }
        if let Some(temp) = sensor.temperature_as_millicelsius() {
            let temp = f64::from(temp) / 1000.0;
            span.record("temperature", temp);
            TEMPERATURE.with_label_values(&[&mac]).set(temp);
        }

        if let Some(volts) = sensor.battery_potential_as_millivolts() {
            let volts = f64::from(volts) / 1000.0;
            span.record("volts", volts);
            POTENTIAL.with_label_values(&[&mac]).set(volts);
        }
        if let Some(txpow) = sensor.tx_power_as_dbm() {
            span.record("txpow", txpow);
            TXPOW.with_label_values(&[&mac]).set(i64::from(txpow));
        }

        if let Some(movement) = sensor.movement_counter() {
            let movement = i64::from(movement);
            span.record("movement", movement);
            MOVEMENT.with_label_values(&[&mac]).set(movement);
        }
        if let Some(accel) = sensor.acceleration_vector_as_milli_g() {
            let accel_x = f64::from(accel.0) / 1000.0;
            let accel_y = f64::from(accel.1) / 1000.0;
            let accel_z = f64::from(accel.2) / 1000.0;
            span.record("accel_x", accel_x);
            span.record("accel_y", accel_y);
            span.record("accel_z", accel_z);
            ACCEL.with_label_values(&[&mac, "x"]).set(accel_x);
            ACCEL.with_label_values(&[&mac, "y"]).set(accel_y);
            ACCEL.with_label_values(&[&mac, "z"]).set(accel_z);
        }
        if let Some(measure_no) = sensor.measurement_sequence_number() {
            span.record("measure_no", measure_no);
            // It is supposed to be a counter, but a ruuvitag may be restarted while the prometheus
            // logger is running, and then we would go backwards, so it is implemented as a value
            // here.
            MEASURE_NO.with_label_values(&[&mac]).set(measure_no.into());
        }
        info!(message = "New measurement");
        drop(enter);
    }
}

mod serve {
    #![forbid(unsafe_code)]
    use std::net::SocketAddr;

    use hyper::http::Error;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Method, Request, Response, Server};
    use lazy_static::lazy_static;
    use prometheus::{register_histogram, register_int_counter, register_int_gauge};
    use prometheus::{Histogram, IntCounter, IntGauge};
    use tracing::{debug, error, info};

    lazy_static! {
        static ref HTTP_COUNTER: IntCounter = register_int_counter!(
            "ruuvi_exporter_requests_total",
            "Number of HTTP requests received."
        )
        .expect("Could not create HTTP_COUNTER metric. This should never fail.");
        static ref HTTP_BODY_GAUGE: IntGauge = register_int_gauge!(
            "ruuvi_exporter_response_size_bytes",
            "The HTTP response sizes in bytes."
        )
        .expect("Could not create HTTP_BODY_GAUGE metric. This should never fail.");
        static ref HTTP_REQ_HISTOGRAM: Histogram = register_histogram!(
            "ruuvi_exporter_request_duration_seconds",
            "The HTTP request latencies in seconds."
        )
        .expect("Could not create HTTP_REQ_HISTOGRAM metric. This should never fail.");
    }

    #[tracing::instrument]
    fn prometheus_to_response() -> Result<Response<Body>, Error> {
        let metric_families = prometheus::gather();
        debug!(
            message = "Generating text output for metrics",
            count = metric_families.len()
        );
        let encoder = prometheus::TextEncoder::new();
        let resp = match encoder.encode_to_string(&metric_families) {
            Ok(body_text) => {
                HTTP_BODY_GAUGE.set(body_text.len() as i64);
                let body = Body::from(body_text);
                Response::new(body)
            }
            Err(error) => {
                error!(message = "Error serializing prometheus data", err = %error);
                let body = Body::from("Error serializing to prometheus");
                Response::builder().status(500).body(body)?
            }
        };
        Ok(resp)
    }

    fn redirect() -> Result<Response<Body>, Error> {
        debug!(
            message = "Redirecting user",
            location = "/metrics",
            status = 301
        );
        let body = Body::from("Go to /metrics \n");
        let resp = Response::builder()
            .status(301)
            .header("Location", "/metrics")
            .body(body)?;
        Ok(resp)
    }

    fn four_oh_four() -> Result<Response<Body>, Error> {
        debug!(message = "Redirecting user", status = 404);
        let resp = Response::builder().status(404).body(Body::empty())?;
        Ok(resp)
    }

    async fn router(req: Request<Body>) -> Result<Response<Body>, Error> {
        HTTP_COUNTER.inc();
        let timer = HTTP_REQ_HISTOGRAM.start_timer();
        let resp = match (req.method(), req.uri().path()) {
            (&Method::GET, "/metrics") => prometheus_to_response()?,
            (&Method::GET, "/") => redirect()?,
            _ => four_oh_four()?,
        };
        drop(timer);
        // This yield is really only to make clippy pedantic mode happy,
        // as this function had no await statements in it.
        tokio::task::yield_now().await;
        Ok(resp)
    }

    #[tracing::instrument]
    pub(crate) async fn webserver(addr: SocketAddr) {
        // For every connection, we must make a `Service` to handle all
        // incoming HTTP requests on said connection.
        let make_svc = make_service_fn(|_conn| {
            // This is the `Service` that will handle the connection.
            // `service_fn` is a helper to convert a function that
            // returns a Response into a `Service`.
            async { Ok::<_, hyper::Error>(service_fn(router)) }
        });

        let server = match Server::try_bind(&addr) {
            Ok(server) => {
                info!(message = "Listening on ", %addr);
                server
            }
            Err(e) => {
                error!(message = "Failed to bind port", err = ?e);
                return;
            }
        };

        match server.serve(make_svc).await {
            Ok(_) => {
                info!("Happy webserver ending");
            }
            Err(e) => {
                error!(message = "Webserver failure", err = ?e);
            }
        }
    }
}
#[derive(Clone)]
struct DeviceHash<'device> {
    inner: Arc<Mutex<HashMap<OwnedObjectPath, MyDev<'device>>>>,
}
impl<'device> DeviceHash<'device> {
    fn new() -> Self {
        // Mapping of  the device path => device
        // Used so we can remove device proxies that are out of date.
        let inner = Arc::new(Mutex::new(HashMap::<OwnedObjectPath, MyDev>::new()));
        Self { inner }
    }
    fn remove(&self, object_path: &OwnedObjectPath) -> Option<MyDev<'device>> {
        let mut devices = self
            .inner
            .lock()
            .inspect_err(|err| error!(message = "Failed to lock devices", err=?err))
            .ok()?;
        devices.remove(object_path)
    }
    fn insert(
        &self,
        object_path: OwnedObjectPath,
        device: MyDev<'device>,
    ) -> Option<MyDev<'device>> {
        let mut devices = self
            .inner
            .lock()
            .inspect_err(|err| error!(message = "Failed to lock devices ", err=?err))
            .ok()?;
        devices.insert(object_path, device)
    }
}

mod mybus {
    #![forbid(unsafe_code)]
    use super::DeviceHash;
    use crate::bluez::adapter1::Adapter1Proxy;
    use crate::bluez::device1::Device1Proxy;
    use crate::from_manuf;
    use ruuvi_sensor_protocol::SensorValues;
    use std::collections::HashMap;
    use std::convert::From;
    use std::fmt;
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;
    use tracing::{debug, error, info, trace, warn};
    use zbus::zvariant::OwnedObjectPath;
    use zbus::ProxyDefault; // Trait

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

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn device_lost(
        mut removed: zbus::fdo::InterfacesRemovedStream<'_>,
        pmap: DeviceHash<'static>,
    ) {
        while let Some(v) = removed.next().await {
            if let Ok(data) = v.args() {
                let object_path = OwnedObjectPath::from(data.object_path);
                // Having the debug line below forces us to de-serialize the entire rest even when
                // we don't care about it.
                // debug!(message = "DBus Interface removed", data = ?data);

                if let Some(device) = pmap.remove(&object_path) {
                    info!(message = "Device disconnected", device = %device);
                    device.bye().await;
                }
            }
        }
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
        async fn bye(self) {
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
            write!(f, "dbus_path={path})")
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn device_found(
        mut added: zbus::fdo::InterfacesAddedStream<'_>,
        pmap: DeviceHash<'static>,
        connection: zbus::Connection,
        tx: mpsc::Sender<SensorValues>,
    ) {
        // Data looks like this:
        // Interfaces Added: data=InterfacesAdded { object_path: ObjectPath("/org/bluez/hci0/dev_C4_47_33_92_F2_96"), interfaces_and_properties: {"org.freedesktop.DBus.Introspectable": {}, "org.bluez.Device1": {"Alias": Str(Str(Borrowed("C4-47-33-92-F2-96"))), "Trusted": Bool(false), "Connected": Bool(false), "Adapter": ObjectPath(ObjectPath("/org/bluez/hci0")), "UUIDs": Array(Array { element_signature: Signature("s"), elements: [], signature: Signature("as") }), "Paired": Bool(false), "AddressType": Str(Str(Borrowed("random"))), "Blocked": Bool(false), "ServicesResolved": Bool(false), "Address": Str(Str(Borrowed("C4:47:33:92:F2:96"))), "Bonded": Bool(false), "AdvertisingFlags": Array(Array { element_signature: Signature("y"), elements: [U8(0)], signature: Signature("ay") }), "LegacyPairing": Bool(false), "ManufacturerData": Dict(Dict { entries: [DictEntry { key: U16(76), value: Value(Array(Array { element_signature: Signature("y"), elements: [U8(18), U8(2), U8(0), U8(2)], signature: Signature("ay") })) }], key_signature: Signature("q"), value_signature: Signature("v"), signature: Signature("a{qv}") }), "RSSI": I16(-77)}, "org.freedesktop.DBus.Properties": {}} }
        while let Some(v) = added.next().await {
            if let Ok(data) = v.args() {
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
                        if let Some(old) = pmap.insert(object_path, device) {
                            info!(message = "Dropping old device for path", old = ?old);
                            old.bye().await;
                        }
                    }
                }
            }
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

async fn real_main() -> Result<(), Box<dyn Error>> {
    let mut connection = zbus::Connection::system().await?;
    connection.set_max_queued(1200);
    let pmap = DeviceHash::new();
    let (tx, rx) = mpsc::channel(100);

    info!(message = "Setting up interface add and remove signals");
    // Lets try to get some changes on the devices
    let bluez_root = zbus::fdo::ObjectManagerProxy::builder(&connection)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;

    let added = bluez_root.receive_interfaces_added().await?;
    let removed = bluez_root.receive_interfaces_removed().await?;

    let address: SocketAddr = "0.0.0.0:9185".parse()?;

    let tasks = [
        // Spawn event listeners for new and removed devices
        tokio::spawn(device_lost(removed, pmap.clone())),
        tokio::spawn(device_found(
            added,
            pmap.clone(),
            connection.clone(),
            tx.clone(),
        )),
        // Web server
        tokio::spawn(serve::webserver(address)),
        // Sensor data processor
        tokio::spawn(prom::sensor_processor(rx)),
    ];
    // Spawn event-listeners for all currently visible devices.

    for dev_proxy in find_devices(&connection).await? {
        let object_path = OwnedObjectPath::from(dev_proxy.path().to_owned());
        let device = MyDev::new(dev_proxy, tx.clone()).await?;
        pmap.insert(object_path, device);
    }

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
