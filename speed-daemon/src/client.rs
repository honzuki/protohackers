use std::{future::pending, time::Duration};

use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::{
        tcp::{ReadHalf, WriteHalf},
        TcpStream,
    },
    sync::{mpsc, oneshot},
};

use crate::{
    protocol::{
        deserializer::{Deserialize, DeserializeError},
        message::{FromClient, ToClient},
        serializer::Serialize,
    },
    systems::{record::CameraHandler, CameraPosition},
    SharedSystems,
};

const TO_CLIENT_BUFFER_SIZE: usize = 32;

type ConnWriter<'a> = BufWriter<WriteHalf<'a>>;
type ConnReader<'a> = BufReader<ReadHalf<'a>>;

pub async fn handle(mut connection: TcpStream, systems: SharedSystems) -> anyhow::Result<()> {
    let (reader, writer) = connection.split();
    let reader = BufReader::new(reader);
    let writer = BufWriter::new(writer);

    let (to_client, rx) = mpsc::channel(TO_CLIENT_BUFFER_SIZE);
    let managed_writer = managed_writer(writer, rx);

    // Create future for each of the sub-systems
    let (set_heartbeat, rx) = oneshot::channel();
    let heartbeat = heartbeat(to_client.clone(), rx);

    let from_client_fut = from_client(reader, to_client, systems, Some(set_heartbeat));

    // run all sub-systems until any exits
    // we can't use select! because we need to allow managed_writer to try and clean
    // its buffer even in a situation where the 'from_client_fut' reached an error an returned.
    let (r1, r2, r3) = tokio::join!(managed_writer, heartbeat, from_client_fut);
    r1?;
    r2?;
    r3
}

async fn managed_writer(
    mut writer: ConnWriter<'_>,
    mut from_server: mpsc::Receiver<ToClient>,
) -> anyhow::Result<()> {
    // forward all messages on the mpsc to the writer part of the socket
    while let Some(message) = from_server.recv().await {
        let mut writer = BufWriter::new(&mut writer);
        message.serialize(&mut writer).await?;
        writer.flush().await?;
    }

    Ok(())
}

async fn heartbeat(
    to_client: mpsc::Sender<ToClient>,
    rx: oneshot::Receiver<f64>,
) -> anyhow::Result<()> {
    let duration = match rx.await {
        Ok(secs) => Duration::from_secs_f64(secs),
        // the client has asked for no heartbeasts
        Err(_) => pending().await,
    };

    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        to_client.send(ToClient::heartbeat()).await?;
    }
}

enum Mode {
    Unregistered(SharedSystems),
    Camera(CameraPosition, CameraHandler),
    Dispatcher,
}

// handle incoming messages from the client
async fn from_client(
    mut reader: ConnReader<'_>,
    to_client: mpsc::Sender<ToClient>,
    systems: SharedSystems,
    mut set_heartbeat: Option<oneshot::Sender<f64>>,
) -> anyhow::Result<()> {
    let mut mode = Mode::Unregistered(systems);

    loop {
        // extract the message
        let message = match FromClient::deserialize(&mut reader).await {
            Ok(message) => message,
            Err(reason) => {
                let reason = match reason {
                    DeserializeError::Io(_) => return Ok(()), // client disconnected
                    DeserializeError::Utf(_) => "invalid string format".into(),
                    DeserializeError::UnknownType(_) => "unknown message".into(),
                };
                to_client.send(ToClient::error(reason)).await?;

                return Ok(());
            }
        };

        match message {
            FromClient::WantHeartbeat { interval } => {
                if let Some(tx) = set_heartbeat.take() {
                    if interval > 0 {
                        tx.send((interval as f64) / 10f64).unwrap();
                    }
                } else {
                    to_client
                        .send(ToClient::error(
                            "the heartbeat interval has already been set".into(),
                        ))
                        .await?;

                    return Ok(());
                }
            }
            FromClient::IAmCamera { road, mile, limit } => {
                if let Mode::Unregistered(systems) = mode {
                    let camera_handler = systems.record.register_camera(road, limit).await;
                    mode = Mode::Camera(mile, camera_handler);
                } else {
                    to_client
                        .send(ToClient::error(
                            "the client has already identified itself".into(),
                        ))
                        .await?;

                    return Ok(());
                }
            }
            FromClient::IAmDispatcher { roads } => {
                if let Mode::Unregistered(mut systems) = mode {
                    systems
                        .ticket
                        .register_dispatcher(roads, to_client.clone())
                        .await;

                    mode = Mode::Dispatcher;
                } else {
                    to_client
                        .send(ToClient::error(
                            "the client has already identified itself".into(),
                        ))
                        .await?;

                    return Ok(());
                }
            }
            FromClient::Plate { plate, timestamp } => {
                if let Mode::Camera(mile, handler) = &mut mode {
                    handler.submit_record(*mile, plate, timestamp).await;
                } else {
                    to_client
                        .send(ToClient::error(
                            "the client has not identified itself as a camera".into(),
                        ))
                        .await?;

                    return Ok(());
                }
            }
        }
    }
}
