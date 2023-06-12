use crate::data::DecodedSensor;
use anyhow::Context;
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
    ) -> anyhow::Result<Self> {
        // modio connection stuff
        let session = std::env::args().any(|arg| arg == "--session");
        let connection = if session {
            zbus::Connection::session().await?
        } else {
            zbus::Connection::system().await?
        };
        Ok(SensorActor {
            receiver,
            sender,
            connection,
        })
    }

    // No point in logging the raw sensor-values at anything but trace level.
    #[tracing::instrument(skip(self), level = "trace")]
    pub async fn handle_message(&mut self, msg: SensorValues) -> anyhow::Result<()> {
        modio_log_sensor(&self.connection, &msg)
            .await
            .with_context(|| "Failed to commmunicate with modio-logger. Is it running?")?;
        self.sender
            .send(msg)
            .await
            .with_context(|| "Failed to pass message on to next listener")?;
        Ok(())
    }
}

#[tracing::instrument(skip_all)]
pub async fn run_sensor_actor(mut actor: SensorActor) -> anyhow::Result<()> {
    info!("Prepared to store data to modio-logger");
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg).await?;
    }
    let err = anyhow::Error::msg("No more sensor values");
    Err(err)
}

#[tracing::instrument(skip_all)]
async fn modio_log_sensor(
    connection: &zbus::Connection,
    sensor: &SensorValues,
) -> anyhow::Result<()> {
    let ipc = fsipc::legacy::fsipcProxy::builder(connection)
        .build()
        .await?;
    let decoder = DecodedSensor::new(sensor);
    if let Some(mac) = decoder.mac() {
        for (name, val, unit) in decoder {
            let key = format!("ruuvi.{mac}.{name}");
            debug!(
                key = key,
                value = val,
                name = name,
                unit = unit,
                "Storing metric"
            );
            ipc.store(&key, &val)
                .await
                .with_context(|| "fsipcProxy store gave error")?;
        }
    }
    Ok(())
}
