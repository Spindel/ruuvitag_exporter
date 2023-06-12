use crate::addr;
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

    static ref COUNT: IntCounterVec =
        register_int_counter_vec!(opts!("signals", "The amount of processed ruuvi signals"), &["mac"]).expect("Failed to initialize");

}

pub fn register_sensor(sensor: &SensorValues) {
    // Traits
    use ruuvi_sensor_protocol::{
        Acceleration, BatteryPotential, Humidity, MacAddress, MovementCounter, Pressure,
        Temperature, TransmitterPower,
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
        accel_z = field::Empty
    );
    let enter = span.enter();

    let mac = if let Some(mac) = sensor.mac_address() {
        addr::Address::from(mac).to_string()
    } else {
        warn!(sensor = ?sensor, "Cannot process sensor");
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
    info!(message = "New measurement");
    drop(enter);
}

pub struct SensorActor {
    receiver: mpsc::Receiver<SensorValues>,
    sender: mpsc::Sender<SensorValues>,
}
impl SensorActor {
    pub fn new(receiver: mpsc::Receiver<SensorValues>, sender: mpsc::Sender<SensorValues>) -> Self {
        SensorActor { receiver, sender }
    }
    pub async fn handle_message(
        &mut self,
        msg: SensorValues,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<SensorValues>> {
        register_sensor(&msg);
        self.sender.send(msg).await
    }
}
pub async fn run_sensor_actor(mut actor: SensorActor) -> anyhow::Result<()> {
    info!("Logging data to prometheus");
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg).await?
    }
    let err = anyhow::Error::msg("No more sensor values");
    Err(err)
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
                Some(_sensor) => {
                    // register_sensor(&sensor);
                    // modio_log_sensor(true, &sensor).await;
                }
                // None happens when there are no senders left.
                None => {
                    warn!("No data-emitter tasks left.");
                    return;
                }
            },
        }
    }
}
