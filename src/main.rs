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

    let mut connection = Connection::system().await?;
    connection.set_max_queued(25600);

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
    use std::convert::From;
    use std::sync::{Arc, Mutex};
    use zbus::zvariant::OwnedValue;
    type Properties = HashMap<String, OwnedValue>;
    use zbus::fdo::PropertiesProxy;
    use zbus::zvariant::OwnedObjectPath;
    use zbus::zvariant::Type;
    use zbus::ProxyDefault; // Trait // Trait?
    /// Get a list of all Bluetooth devices which have been discovered so far.
    async fn find_devices<'device>(
        connection: &zbus::Connection,
    ) -> zbus::Result<HashMap<OwnedObjectPath, Device1Proxy<'device>>> {
        let bluez_root = zbus::fdo::ObjectManagerProxy::builder(&connection)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await?;
        let tree = bluez_root.get_managed_objects().await?;

        // Filter down to only the pairs that match our interface
        let devices: Vec<(OwnedObjectPath, Properties)> = tree
            .into_iter()
            .filter_map(|(object_path, mut children)| {
                match children.remove(Device1Proxy::INTERFACE) {
                    Some(data) => Some((object_path, data)),
                    None => None,
                }
            })
            .collect();

        // Return the mappings of proxy types + properties in case someone cares.
        let mut result: Vec<(OwnedObjectPath, Device1Proxy)> = Vec::new();
        for (object_path, blob) in devices {
            let device = Device1Proxy::builder(&connection)
                .destination("org.bluez")?
                .path(object_path.clone())?
                .cache_properties(zbus::CacheProperties::Yes)
                .build()
                .await?;

            println!(
                "dev adapter= {:?}  addr={}",
                device.adapter().await?,
                device.address().await?
            );
            if let Ok(manuf_data) = device.manufacturer_data().await {
                //println!("manuf_data conversion: {:?}", &manuf_data);
                if let Some(sens) = from_manuf(manuf_data) {
                    println!("Ruuvi data {:?}", sens);
                }
            };
            /*
             * Example of how to call the get_property and type-convert it without having a direct proxy
                        if let Ok(rssi) = device.get_property::<i16>("RSSI").await {
                            println!("rssi={:?}", rssi);
                        };
            */
            result.push((object_path, device));
        }
        let out = HashMap::from_iter(result);
        Ok(out)
    }

    let mut adapters = find_adapters(&connection).await?;
    let adapter = adapters.pop().unwrap();
    adapter.set_powered(true).await?;
    // discovery filter is a map str=>val with fixed keys
    // see
    //  https://github.com/bluez-rs/bluez-async/blob/main/bluez-async/src/lib.rs#L143
    // and
    //  https://github.com/Vudentz/BlueZ/blob/master/doc/adapter-api.txt#L49 for docs
    //
    adapter.set_discovery_filter(HashMap::new()).await?;
    adapter.start_discovery().await?;

    let addr = adapter.address().await?;
    println!("We have a hci address: {}", addr);

    let mut devices = find_devices(&connection).await?;

    for (opath, dev) in &devices {
        //        let stream = dev.receive_all_signals().await?;
        let path = dev.path().to_owned();
        let dest = dev.destination().to_owned();
        println!("{}, {}", &path, &dest);
        /*
        let props = zbus::fdo::PropertiesProxy::builder(&connection)
            .destination("org.bluez")?
            .cache_properties(zbus::CacheProperties::Yes)
            .path(path)?
            .build()
            .await?;
        println!("device={:?}, {:?}", props.path(), props.interface());
        for (k, v) in props.get_all(dev.interface().to_owned()).await? {
            println!("Property {:?} == {:?}", k, v);
        }*/
        //
        //let mut stream = props.receive_properties_changed().await?;
        //        println!("Meeeh got stream");
        //        let stream = dev.receive_rssi_changed().await;
        //        let key = dev.path().to_owned();
        //        prop_stream.insert(key, stream);
    }

    let (task_write, mut task_read) = mpsc::channel(100);
    println!("Meeeh, no stream");
    for (opath, dev) in &devices {
        let path = dev.path().to_owned();
        let dest = dev.destination().to_owned();
        let iface = dev.interface().to_owned();

        let devaddr = dev.address().await?;
        let devname = dev.name().await.ok();
        println!(
            "Setting up receive manufacturer_data_changed signal, device={:?}, dest={:?}, iface={:?} name={:?} addr={:?}",
            path, dest, iface, devname, devaddr
        );
        //let stream = dev.receive_all_signals().await?;
        //        let stream = dev.receive_rssi_changed().await;
        let stream = dev.receive_manufacturer_data_changed().await;
        let key = dev.path().to_owned();
        //dbg!(&stream);
        task_write
            .send(tokio::spawn(manufacturer_listener(stream)))
            .await
            .unwrap();
    }
    println!("meeh about to manage some proies");

    // Lets try to get some changes on the devices
    let bluez_root = zbus::fdo::ObjectManagerProxy::builder(&connection)
        .destination("org.bluez")?
        .path("/")?
        .build()
        .await?;
    let mut added = bluez_root.receive_interfaces_added().await?;
    let mut removed = bluez_root.receive_interfaces_removed().await?;
    //   let mut allsig = dbus_proxy.receive_all_signals().await?;

    let pmap = Arc::new(Mutex::new(devices));

    async fn singals_drop_device(
        mut removed: zbus::fdo::InterfacesRemovedStream<'_>,
        pmap: Arc<Mutex<HashMap<OwnedObjectPath, Device1Proxy<'_>>>>,
    ) {
        let pmap = pmap.clone();
        while let Some(v) = removed.next().await {
            // println!("Something happened: {:?}", v);
            if let Ok(data) = v.args() {
                println!("Interfaces Removed: data={:?}", &data);
                let path = data.object_path;
                let hashpath = OwnedObjectPath::from(path.as_ref());
                let mut devices = pmap.lock().unwrap();

                if let Some(dev) = devices.get(&hashpath) {
                    // Only drop the device _if_ the interface we use is amongst the dropped.
                    if data.interfaces.contains(&dev.interface().as_str()) {
                        if let Some(dev) = devices.remove(&hashpath) {
                            println!("Removed this dev {:?}", dev.path().as_str());
                            //println!("Removed this dev {:?}", dev);
                        } else {
                            println!("Failed to remove"); // {:?}", dev);
                        }
                    }
                }
            }
        }
    }

    async fn signals_add_device(
        mut added: zbus::fdo::InterfacesAddedStream<'_>,
        pmap: Arc<Mutex<HashMap<OwnedObjectPath, Device1Proxy<'_>>>>,
        connection: zbus::Connection,
        mut task_write: mpsc::Sender<tokio::task::JoinHandle<()>>,
    ) {
        //                    Interfaces Added: data=InterfacesAdded { object_path: ObjectPath("/org/bluez/hci0/dev_C4_47_33_92_F2_96"), interfaces_and_properties: {"org.freedesktop.DBus.Introspectable": {}, "org.bluez.Device1": {"Alias": Str(Str(Borrowed("C4-47-33-92-F2-96"))), "Trusted": Bool(false), "Connected": Bool(false), "Adapter": ObjectPath(ObjectPath("/org/bluez/hci0")), "UUIDs": Array(Array { element_signature: Signature("s"), elements: [], signature: Signature("as") }), "Paired": Bool(false), "AddressType": Str(Str(Borrowed("random"))), "Blocked": Bool(false), "ServicesResolved": Bool(false), "Address": Str(Str(Borrowed("C4:47:33:92:F2:96"))), "Bonded": Bool(false), "AdvertisingFlags": Array(Array { element_signature: Signature("y"), elements: [U8(0)], signature: Signature("ay") }), "LegacyPairing": Bool(false), "ManufacturerData": Dict(Dict { entries: [DictEntry { key: U16(76), value: Value(Array(Array { element_signature: Signature("y"), elements: [U8(18), U8(2), U8(0), U8(2)], signature: Signature("ay") })) }], key_signature: Signature("q"), value_signature: Signature("v"), signature: Signature("a{qv}") }), "RSSI": I16(-77)}, "org.freedesktop.DBus.Properties": {}} }

        let pmap = pmap.clone();
        while let Some(v) = added.next().await {
            if let Ok(data) = v.args() {
                //println!("Interfaces Added: data={:?}",  &data);
                let object_path = data.object_path.clone();
                let hashpath = OwnedObjectPath::from(object_path.as_ref()).clone();
                let hashpath2 = OwnedObjectPath::from(object_path.as_ref()).clone();
                let hashpath3 = OwnedObjectPath::from(object_path.as_ref()).clone();

                // Filter down to only the pairs that match our interface
                let devdata = data
                    .interfaces_and_properties
                    .into_iter()
                    .filter_map(|(interface, data)| match interface {
                        Device1Proxy::INTERFACE => Some(data),
                        _ => None,
                    })
                    .next();
                if devdata.is_some() {
                    // Do something here with Data["ManufacturerData"]  to parse it into
                    // something useful
                    let device = Device1Proxy::builder(&connection)
                        .destination("org.bluez")
                        .unwrap()
                        .path(hashpath2)
                        .unwrap()
                        .cache_properties(zbus::CacheProperties::Yes)
                        .build()
                        .await
                        .unwrap();

                    println!("Setting up receive manufacturer_data_changed signal, device={:?}, dest={:?}, iface={:?} name={:?} addr={:?}",
                        device.path(), device.destination(), device.interface(), device.name().await.ok(), device.address().await.ok(),
                    );
                    let stream = device.receive_manufacturer_data_changed().await;
                    // Spawn a task and drop it on the floor.
                    // Maybe we should hand it off to something else later on
                    task_write
                        .send(tokio::spawn(manufacturer_listener(stream)))
                        .await
                        .unwrap();
                    let mut devices = pmap.lock().unwrap();
                    if let Some(old) = devices.insert(hashpath, device) {
                        println!("There was alrady a device in there! {:?}", old);
                    }
                }
            }
        }
    }

    async fn manufacturer_listener(mut stream: zbus::PropertyStream<'_, HashMap<u16, Vec<u8>>>) {
        while let Some(v) = stream.next().await {
            //           println!("Something happened: {:?}", v.name());
            // println!("Signal: {:?} payload {:?}", v.name(), payload);
            println!("device Signal: name={}, val={:?}", v.name(), v.get().await);
            if v.name() == "ManufacturerData" {
                if let Ok(val) = v.get().await {
                    if let Some(sens) = from_manuf(val) {
                        println!("Ruuvi data {:?}", sens);
                    }
                }
            }
        }
    }
    let taskinger = task_write.clone();
    tokio::try_join!(
        async move {
            let mut tasks = Vec::new();
            println!("Awaiting tasks");
            while let Some(t) = task_read.recv().await {
                tasks.push(t);
                let sum: usize = tasks
                    .iter()
                    .map(|t| match t.is_finished() {
                        true => 1,
                        false => 0,
                    })
                    .sum();
                println!("New task arrived, tasks={} finished={}", tasks.len(), sum);
            }
            Ok(())
        },
        tokio::spawn(singals_drop_device(removed, pmap.clone())),
        tokio::spawn(signals_add_device(
            added,
            pmap.clone(),
            connection.clone(),
            task_write.clone()
        )),
    )
    .expect("Tried to fail");
    //        async {
    //            while let Some((k, v)) = prop_stream.next().await {
    //                let args = v.args().unwrap();
    //                for (name, value) in args.changed_properties().iter() {
    //                    println!("PropertyChanged: k={} {:?} to {:?}", &k, &name, &value);
    //                }

    //                let payload = v.get().await.unwrap();
    //                println!("prop_stream: {:?} payload {:?}", v.name(), payload);
    //            }
    //        },

    // All done, shut down
    adapter.stop_discovery().await?;

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
