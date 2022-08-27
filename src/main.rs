use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;

mod bluez;
use bluez::adapter1::Adapter1Proxy;
use bluez::device1::Device1Proxy;

use btleplug::api::{Central, CentralEvent, Manager as _, Peripheral};
use btleplug::platform::{Adapter, Manager};

use ruuvi_sensor_protocol::SensorValues;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{info, span, warn, Level};

async fn get_central(manager: &Manager) -> Result<Adapter, btleplug::Error> {
    let adapters = manager.adapters().await?;
    if let Some(adapter) = adapters.into_iter().next() {
        Ok(adapter)
    } else {
        Err(btleplug::Error::DeviceNotFound)
    }
}

fn from_manuf(manufacturer_data: HashMap<u16, Vec<u8>>) -> Option<SensorValues> {
    for (k, v) in manufacturer_data {
        if let Ok(sensor) = SensorValues::from_manufacturer_specific_data(k, &v) {
            return Some(sensor);
        }
    }
    None
}

mod prom {
    use btleplug::api::BDAddr;
    use lazy_static::lazy_static;
    use prometheus::{self, GaugeVec, IntCounterVec, IntGaugeVec};
    use prometheus::{
        labels, opts, register_gauge_vec, register_int_counter_vec, register_int_gauge_vec,
    };
    use ruuvi_sensor_protocol::SensorValues;
    use ruuvi_sensor_protocol::{MacAddress, MeasurementSequenceNumber};
    use std::collections::HashMap;
    use tokio::sync::mpsc;
    use tracing::{info, span, warn, Level};

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
                let mac: BDAddr = mac_bytes.into();
                let inner = seen.entry(mac).or_insert(0u32);
                if *inner != count {
                    *inner = count;
                    // We haven't seen it recently
                    register_sensor(&sensor);
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
            BDAddr::from(mac).to_string()
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

async fn ruuvi_emitter(
    central: Adapter,
    tx: mpsc::Sender<SensorValues>,
) -> Result<(), Box<dyn Error>> {
    use btleplug::api::ScanFilter;
    use std::time::Duration;
    use tokio::time::timeout;
    const MINUTE: Duration = Duration::from_secs(60);

    let mut events = central.events().await?;

    loop {
        let span = span!(Level::TRACE, "ruuvi_emitter");
        let enter = span.enter();
        info!(message = "Starting scan on", adapter = ?central, timeout = ?MINUTE);
        central.start_scan(ScanFilter::default()).await?;
        // We use the timeout to restart the scan if we don't see any events
        while let Ok(Some(event)) = timeout(MINUTE, events.next()).await {
            match event {
                CentralEvent::ManufacturerDataAdvertisement {
                    id: _,
                    manufacturer_data,
                } => {
                    if let Some(sensor) = from_manuf(manufacturer_data) {
                        tx.send(sensor).await?;
                    }
                }
                CentralEvent::DeviceDiscovered(id) | CentralEvent::DeviceUpdated(id) => {
                    if let Ok(peripheral) = &central.peripheral(&id).await {
                        if let Ok(Some(props)) = peripheral.properties().await {
                            if let Some(sensor) = from_manuf(props.manufacturer_data) {
                                tx.send(sensor).await?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        warn!(message = "No new events, restarting scanner");
        drop(enter);
        central.stop_scan().await?;
    }
}
use zbus::Connection;
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let connection = Connection::system().await?;

    async fn find_adapters(connection: &zbus::Connection) -> zbus::Result<Vec<Adapter1Proxy>> {
        use zbus::ProxyDefault;
        let mut result: Vec<Adapter1Proxy> = Vec::new();

        let p = zbus::fdo::ObjectManagerProxy::builder(&connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let managed = p.get_managed_objects().await?;

        for (path, children) in &managed {
            for (interface, _props) in children {
                // This will print the found paths, devices and their metadata
                // println!("path={:?} interface={} muh={:?}", &path, interface, _props);
                if interface.as_str() == Adapter1Proxy::INTERFACE {
                    let adapter = Adapter1Proxy::builder(&connection)
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

    use zbus::zvariant::OwnedValue;
    type Properties = HashMap<String, OwnedValue>;
    /// Get a list of all Bluetooth devices which have been discovered so far.
    async fn find_devices(
        connection: &zbus::Connection,
    ) -> zbus::Result<Vec<(Device1Proxy, Properties)>> {
        use zbus::ProxyDefault;
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(&connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let tree = bluez_root.get_managed_objects().await?;
        use zbus::zvariant::OwnedObjectPath;

        // Filter down to only the pairs that match our interface
        let devices: Vec<(OwnedObjectPath, Properties)> = tree
            .into_iter()
            .filter_map(|(object_path, mut children)| {
                match children.remove(Device1Proxy::INTERFACE) {
                    Some(data) => Some((object_path.clone(), data)),
                    None => None,
                }
            })
            .collect();

        // Return the mappings of proxy types + properties in case someone cares.
        let mut result: Vec<(Device1Proxy, Properties)> = Vec::new();
        for (object_path, blob) in devices {
            let device = Device1Proxy::builder(&connection)
                .destination("org.bluez")?
                .path(object_path)?
                .build()
                .await?;
            result.push((device, blob));
        }
        Ok(result)
    }

    let mut adapters = find_adapters(&connection).await?;
    let adapter = adapters.pop().unwrap();

    let addr = adapter.address().await?;
    println!("We have a hci address: {}", addr);

    let _devices = find_devices(&connection).await?;

    let manager = Manager::new().await?;

    // Get the first Bluetooth adapter
    // connect to the adapter
    let central = get_central(&manager).await?;

    let (tx, rx) = mpsc::channel(100);
    let address: SocketAddr = "0.0.0.0:9185".parse()?;

    // Tasks we want to clean up when we're done.
    let to_kill = vec![
        tokio::spawn(prom::sensor_processor(rx)),
        tokio::spawn(serve::webserver(address)),
    ];

    // When the event listener exits, we terminate.
    let res = ruuvi_emitter(central.clone(), tx).await;
    // Stop scanning so we don't leave the adapter in scan mode.
    central.stop_scan().await?;
    for t in to_kill {
        t.abort();
        drop(t);
    }
    res
}
