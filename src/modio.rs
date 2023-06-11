use crate::data::DecodedSensor;
use ruuvi_sensor_protocol::SensorValues;
use tokio::sync::mpsc;
use tracing::{debug, info};

pub struct SensorActor {
    receiver: mpsc::Receiver<SensorValues>,
    sender: mpsc::Sender<SensorValues>,
    connection: zbus::Connection,
}
impl SensorActor {
    pub async fn new(
        receiver: mpsc::Receiver<SensorValues>,
        sender: mpsc::Sender<SensorValues>,
    ) -> Self {
        // modio connection stuff
        let session = std::env::args().any(|arg| arg == "--session");
        let connection = if session {
            zbus::Connection::session().await.unwrap()
        } else {
            zbus::Connection::system().await.unwrap()
        };
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
    info!("Prepared to store data to modio-logger");
    while let Some(msg) = actor.receiver.recv().await {
        actor
            .handle_message(msg)
            .await
            .expect("Unknown failure in channel");
    }
}

#[tracing::instrument(skip_all)]
async fn modio_log_sensor(connection: &zbus::Connection, sensor: &SensorValues) {
    let ipc = fsipc::legacy::fsipcProxy::builder(connection)
        .build()
        .await
        .unwrap();
    let decoder = DecodedSensor::new(sensor);
    if let Some(mac) = decoder.mac() {
        for (name, val, unit) in decoder {
            debug!(value = val, name = name, unit = unit, "Decoded value");
            let key = format!("ruuvi.{mac}.{name}");
            ipc.store(&key, &val).await.unwrap();
        }
    }
}
