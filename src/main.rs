use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;

mod addr;
mod bluez;

use ruuvi_sensor_protocol::SensorValues;
use tokio_stream::StreamExt;
use tracing::info;

fn from_manuf(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<SensorValues> {
    for (k, v) in manufacturer_data {
        if let Ok(sensor) = SensorValues::from_manufacturer_specific_data(k, &v) {
            return Some(sensor);
        }
    }
    None
}

mod prom {
    use lazy_static::lazy_static;
    use prometheus::{self, GaugeVec, IntCounterVec, IntGaugeVec};
    use prometheus::{
        labels, opts, register_gauge_vec, register_int_counter_vec, register_int_gauge_vec,
    };
    use ruuvi_sensor_protocol::SensorValues;
    use ruuvi_sensor_protocol::{MacAddress, MeasurementSequenceNumber};
    use std::collections::HashMap;
    use tokio::sync::mpsc;
    use tracing::{error, info, span, warn, Level};

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

        static ref COUNT: IntCounterVec =
            register_int_counter_vec!(opts!("signals", "The amount of processed ruuvi signals"), &["mac"]).expect("Failed to initialize");

    }
    use crate::addr;

    pub(crate) async fn sensor_processor(mut rx: mpsc::Receiver<SensorValues>) {
        let mut seen = HashMap::new();
        // Because we get duplicate values from ble (both via discovery and other methods) we want
        // to keep track of the ones we last saw.
        //
        // This means we keep a hashmap mapping the mac address to the sequence number, and if the
        // two match, will register it with prometheus. Otherwise we just ignore it.

        while let Some(sensor) = rx.recv().await {
            let span = span!(Level::TRACE, "sensor_processing").entered();
            if let (Some(mac_bytes), Some(count)) =
                (sensor.mac_address(), sensor.measurement_sequence_number())
            {
                let mac = addr::Address::from(mac_bytes);
                let inner = seen.entry(mac).or_insert(0u32);
                if *inner != count {
                    *inner = count;
                    // We haven't seen it recently
                    register_sensor(&sensor);
                } else {
                    error!(message = "Got a dupe", count = ?count, mac = %mac);
                }
            }
            drop(span);
        }
    }

    fn register_sensor(sensor: &SensorValues) {
        use ruuvi_sensor_protocol::{
            Acceleration, BatteryPotential, Humidity, MovementCounter, Pressure, Temperature,
            TransmitterPower,
        };
        use tracing::field;
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
            accel_z = field::Empty
        );
        let enter = span.enter();

        let mac = if let Some(mac) = sensor.mac_address() {
            addr::Address::from(mac).to_string()
        } else {
            warn!("Cannot process sensor: {:?}", sensor = sensor);
            return;
        };
        // Tracing has a hard time with String, this wraps it as a borrowed value
        span.record("mac", &tracing::field::display(&mac));

        COUNT.with_label_values(&[&mac]).inc();
        if let Some(humidity) = sensor.humidity_as_ppm() {
            let humidity = f64::from(humidity) / 10000.0;
            span.record("humidity", &humidity);
            HUMIDITY.with_label_values(&[&mac]).set(humidity);
        }

        if let Some(pressure) = sensor.pressure_as_pascals() {
            let pressure = f64::from(pressure) / 1000.0;
            span.record("pressure", &pressure);
            PRESSURE.with_label_values(&[&mac]).set(pressure);
        }
        if let Some(temp) = sensor.temperature_as_millicelsius() {
            let temp = f64::from(temp) / 1000.0;
            span.record("temperature", &temp);
            TEMPERATURE.with_label_values(&[&mac]).set(temp);
        }

        if let Some(volts) = sensor.battery_potential_as_millivolts() {
            let volts = f64::from(volts) / 1000.0;
            span.record("volts", &volts);
            POTENTIAL.with_label_values(&[&mac]).set(volts);
        }
        if let Some(txpow) = sensor.tx_power_as_dbm() {
            span.record("txpow", &txpow);
            TXPOW.with_label_values(&[&mac]).set(i64::from(txpow));
        }

        if let Some(movement) = sensor.movement_counter() {
            let movement = i64::from(movement);
            span.record("movement", &movement);
            MOVEMENT.with_label_values(&[&mac]).set(movement);
        }
        if let Some(accel) = sensor.acceleration_vector_as_milli_g() {
            let accel_x = f64::from(accel.0) / 1000.0;
            let accel_y = f64::from(accel.1) / 1000.0;
            let accel_z = f64::from(accel.2) / 1000.0;
            span.record("accel_x", &accel_x);
            span.record("accel_y", &accel_y);
            span.record("accel_z", &accel_z);
            ACCEL.with_label_values(&[&mac, "x"]).set(accel_x);
            ACCEL.with_label_values(&[&mac, "y"]).set(accel_y);
            ACCEL.with_label_values(&[&mac, "z"]).set(accel_z);
        }
        info!(message = "New measurement");
        drop(enter);
    }
}

mod serve {
    use lazy_static::lazy_static;
    use std::net::SocketAddr;
    use tracing::{debug, error, info};

    use hyper::http::Error;
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Method, Request, Response, Server};
    use prometheus::{register_histogram, register_int_counter, register_int_gauge};
    use prometheus::{Histogram, IntCounter, IntGauge};

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
        let server = Server::bind(&addr).serve(make_svc);
        info!(message = "Listening on ", %addr);
        server.await.expect("Webserver failure");
    }
}

mod mybus {
    use crate::bluez::adapter1::Adapter1Proxy;
    pub use crate::bluez::device1::Device1Proxy;
    use crate::from_manuf;
    use crate::SensorValues;
    use std::collections::HashMap;
    use std::convert::From;
    use std::fmt;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use tokio_stream::StreamExt;
    use tracing::{debug, info, span, Level};
    use zbus::zvariant::OwnedObjectPath;
    use zbus::ProxyDefault; // Trait
    type DeviceHash<'device> = Arc<Mutex<HashMap<OwnedObjectPath, MyDev<'device>>>>;

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
    ) -> zbus::Result<Vec<OwnedObjectPath>> {
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let tree = bluez_root.get_managed_objects().await?;

        // Filter down to only the pairs that match our interface
        let result = tree
            .into_iter()
            .filter_map(|(object_path, mut children)| {
                children
                    .remove(Device1Proxy::INTERFACE)
                    .map(|data| (object_path, data))
            })
            .map(|(object_path, _)| object_path)
            .collect();
        Ok(result)
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn signals_drop_device(
        mut removed: zbus::fdo::InterfacesRemovedStream<'_>,
        pmap: DeviceHash<'_>,
    ) {
        while let Some(v) = removed.next().await {
            if let Ok(data) = v.args() {
                debug!(message = "Interfaces Removed", data = ?data);
                let hashpath = OwnedObjectPath::from(data.object_path);
                let mut devices = pmap.lock().unwrap();
                if let Some(dev) = devices.remove(&hashpath) {
                    info!(message = "Device disconnected", device = %dev);
                }
            }
        }
    }

    #[derive(Debug)]
    pub struct MyDev<'device> {
        device: Device1Proxy<'device>,
        listener: tokio::task::JoinHandle<()>,
    }
    impl MyDev<'_> {
        pub async fn new(
            connection: zbus::Connection,
            tx: mpsc::Sender<SensorValues>,
            object_path: OwnedObjectPath,
        ) -> zbus::Result<MyDev<'static>> {
            let span = span!(Level::INFO, "MyDev::new").or_current();

            let device = Device1Proxy::builder(&connection)
                .destination("org.bluez")?
                .path(object_path.clone())?
                .cache_properties(zbus::CacheProperties::Yes)
                .build()
                .await?;

            span.record("device", device.path().to_string());
            span.record("dest", device.destination().to_string());
            span.record("iface", device.interface().to_string());
            if let Ok(address) = device.address().await {
                span.record("address", address);
            }
            if let Ok(name) = device.name().await {
                span.record("name", name);
            }
            let stream = device.receive_manufacturer_data_changed().await;

            // Should we also grab it directly and log it at once?
            // IF so, the code looks like this.
            //
            if let Ok(manuf_data) = device.manufacturer_data().await {
                if let Some(sens) = from_manuf(manuf_data) {
                    tx.send(sens).await.unwrap();
                }
            };
            // Spawn a task to poll this device's stream
            let listener = tokio::spawn(manufacturer_listener(stream, tx));

            let res = Self { device, listener };
            Ok(res)
        }
        pub async fn name(&self) -> String {
            match self.device.name().await {
                Ok(name) => name,
                Err(_) => String::from(""),
            }
        }
    }
    impl<'device> Drop for MyDev<'device> {
        fn drop(&mut self) {
            //println!("> Aborting listener {:?}", self.listener);
            self.listener.abort();
            /*
            println!(
                "Drop from {} {}",
                self.device.destination(),
                self.device.path()
            );
            */
        }
    }
    impl<'device> fmt::Display for MyDev<'device> {
        // This trait requires `fmt` with this exact signature.
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            // Write strictly the first element into the supplied output
            // stream: `f`. Returns `fmt::Result` which indicates whether the
            // operation succeeded or failed. Note that `write!` uses syntax which
            // is very similar to `println!`.
            write!(
                f,
                "MyDev( {} {}  listener={})",
                self.device.destination(),
                self.device.path(),
                self.listener.is_finished()
            )
        }
    }

    pub async fn signals_add_device(
        mut added: zbus::fdo::InterfacesAddedStream<'_>,
        pmap: DeviceHash<'_>,
        connection: zbus::Connection,
        tx: mpsc::Sender<SensorValues>,
    ) {
        // Data looks like this:
        // Interfaces Added: data=InterfacesAdded { object_path: ObjectPath("/org/bluez/hci0/dev_C4_47_33_92_F2_96"), interfaces_and_properties: {"org.freedesktop.DBus.Introspectable": {}, "org.bluez.Device1": {"Alias": Str(Str(Borrowed("C4-47-33-92-F2-96"))), "Trusted": Bool(false), "Connected": Bool(false), "Adapter": ObjectPath(ObjectPath("/org/bluez/hci0")), "UUIDs": Array(Array { element_signature: Signature("s"), elements: [], signature: Signature("as") }), "Paired": Bool(false), "AddressType": Str(Str(Borrowed("random"))), "Blocked": Bool(false), "ServicesResolved": Bool(false), "Address": Str(Str(Borrowed("C4:47:33:92:F2:96"))), "Bonded": Bool(false), "AdvertisingFlags": Array(Array { element_signature: Signature("y"), elements: [U8(0)], signature: Signature("ay") }), "LegacyPairing": Bool(false), "ManufacturerData": Dict(Dict { entries: [DictEntry { key: U16(76), value: Value(Array(Array { element_signature: Signature("y"), elements: [U8(18), U8(2), U8(0), U8(2)], signature: Signature("ay") })) }], key_signature: Signature("q"), value_signature: Signature("v"), signature: Signature("a{qv}") }), "RSSI": I16(-77)}, "org.freedesktop.DBus.Properties": {}} }
        let span = span!(Level::TRACE, "add_device_signal").or_current();
        while let Some(v) = added.next().await {
            if let Ok(data) = v.args() {
                let object_path = OwnedObjectPath::from(data.object_path);
                span.record("object_path", &object_path.as_str());

                // Filter down to only the pairs that match our interface
                let devdata =
                    data.interfaces_and_properties
                        .into_iter()
                        .find_map(|(interface, data)| match interface {
                            Device1Proxy::INTERFACE => Some(data),
                            _ => None,
                        });
                if devdata.is_some() {
                    // Clone the data so we can just toss it without caring about lifetimes.
                    // The "biggest" is a string for the path.
                    let device = MyDev::new(connection.clone(), tx.clone(), object_path.clone())
                        .await
                        .unwrap();
                    span.record("name", device.name().await);
                    let mut devices = pmap.lock().unwrap();
                    if let Some(old) = devices.insert(object_path, device) {
                        info!(message = "Dropping old device for path", old = ?old);
                    }
                }
            }
        }
    }

    #[tracing::instrument(level = "info", skip_all)]
    pub async fn start_discovery(connection: &zbus::Connection) -> zbus::Result<()> {
        let adapters = find_adapters(&connection).await?;
        for adapter in &adapters {
            let name = adapter.name().await?;

            info!(message = "Powering on", adapter = ?name);
            adapter.set_powered(true).await?;
            // discovery filter is a map str=>val with fixed keys
            // see
            //  https://github.com/bluez-rs/bluez-async/blob/main/bluez-async/src/lib.rs#L143
            // and
            //  https://github.com/Vudentz/BlueZ/blob/master/doc/adapter-api.txt#L49 for docs
            //
            adapter.set_discovery_filter(HashMap::new()).await?;
            info!(message = "Starting discovery on", adapter = ?name);
            adapter.start_discovery().await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn manufacturer_listener(
        mut stream: zbus::PropertyStream<'_, HashMap<u16, Vec<u8>>>,
        tx: mpsc::Sender<SensorValues>,
    ) {
        while let Some(v) = stream.next().await {
            //           println!("Something happened: {:?}", v.name());
            // println!("Signal: {:?} payload {:?}", v.name(), payload);
            let value = v.get().await;
            debug!(message = "Device Signal",  name = ?v.name(),  value = ?value);
            if v.name() == "ManufacturerData" {
                if let Ok(val) = v.get().await {
                    if let Some(sens) = from_manuf(val) {
                        debug!(message = "Ruuvi data",  data = ?sens);
                        tx.send(sens).await.unwrap();
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use mybus::find_adapters;
    use mybus::find_devices;
    use mybus::signals_add_device;
    use mybus::signals_drop_device;
    use mybus::start_discovery;
    use mybus::MyDev;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use zbus::zvariant::OwnedObjectPath;
    use zbus::Connection;

    use futures::stream::FuturesUnordered;
    use tokio::task::JoinHandle;

    tracing_subscriber::fmt::init();
    let mut connection = Connection::system().await?;
    connection.set_max_queued(25600);

    // Mapping of  the device path => device
    // Used so we can remove device proxies that are out of date.
    let pmap = Arc::new(Mutex::new(HashMap::<OwnedObjectPath, MyDev>::new()));

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
        tokio::spawn(signals_drop_device(removed, pmap.clone())),
        tokio::spawn(signals_add_device(
            added,
            pmap.clone(),
            connection.clone(),
            tx.clone(),
        )),
        // Web server and sensor data processor
        tokio::spawn(prom::sensor_processor(rx)),
        tokio::spawn(serve::webserver(address)),
    ];

    // Spawn event-listeners for all currently visible devices.
    for object_path in find_devices(&connection).await? {
        let device = MyDev::new(connection.clone(), tx.clone(), object_path.clone()).await?;
        let mut devices = pmap.lock().unwrap();
        devices.insert(object_path, device);
    }

    start_discovery(&connection).await?;

    let mut futs = tasks
        .into_iter()
        .collect::<FuturesUnordered<JoinHandle<_>>>();
    if let Some(task_result) = futs.next().await {
        println!("Something end ed and I'm not sure what!");
        match task_result {
            Ok(msg) => println!("Task ended succesfully {:?}", msg),
            Err(msg) => println!("Task ended badly {}", msg),
        }
    }
    // All done, shut down
    for adapter in find_adapters(&connection).await? {
        adapter.stop_discovery().await?;
    }
    Ok(())
}
