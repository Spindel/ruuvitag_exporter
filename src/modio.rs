use crate::data::DecodedSensor;
use anyhow::Context;
use ruuvi_sensor_protocol::SensorValues;
use std::collections::BTreeSet;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use fsipc::logger1::Logger1Proxy;

pub struct SensorActor {
    receiver: mpsc::Receiver<SensorValues>,
    sender: mpsc::Sender<SensorValues>,
    connection: zbus::Connection,
    metadata: BTreeSet<String>,
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
        let metadata = BTreeSet::new();
        Ok(SensorActor {
            receiver,
            sender,
            connection,
            metadata,
        })
    }

    // No point in logging the raw sensor-values at anything but trace level.
    #[tracing::instrument(skip(self), level = "trace")]
    pub async fn handle_message(&mut self, msg: SensorValues) -> anyhow::Result<()> {
        let metadata = &mut self.metadata;
        modio_log_sensor(&self.connection, &msg, metadata)
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

// Inner function for log sensor to make the body of the loop neater
async fn update_metadata(ipc: &Logger1Proxy<'_>, mac: &str, key: &str, unit: &str, name: &str) -> anyhow::Result<()> {
    let m_name = format!("Ruuvi: {mac}: {name}");
    info!(key = key, name = m_name, unit = unit, "Updating metadata");
    ipc.set_metadata_name(key, &m_name).await?;
    if let Err(msg) = ipc.set_metadata_unit(key, unit).await {
        warn!(key = key, unit = unit, err = msg.to_string(), "Failed to set unit");
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn modio_log_sensor(
    connection: &zbus::Connection,
    sensor: &SensorValues,
    metadata: &mut BTreeSet<String>,
) -> anyhow::Result<()> {
    use std::collections::HashMap;
    let ipc = Logger1Proxy::builder(connection)
        .build()
        .await?;

    let decoder = DecodedSensor::new(sensor);
    if let Some(mac) = decoder.mac() {
        let mut batch: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();

        for (key, name, val, unit) in decoder {
            if !metadata.contains(&key) {
                update_metadata(&ipc, &mac, &key, &unit, &name).await?;
                metadata.insert(key.clone());
            }
            debug!(key = key, value = ?val, name = name, unit = unit, "Storing metric");
            batch.insert(key, val.into());
        };
        ipc.store(batch)
            .await
            .with_context(|| "Logger1Proxy store gave error")?;
    }
    Ok(())
}
