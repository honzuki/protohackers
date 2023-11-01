use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream},
    net::UdpSocket,
    sync::{mpsc, Mutex},
};

use crate::lrcp::{RETRANSMISSION_TIMEOUT, SESSION_EXPIRY_TIMEOUT};

use super::{message::Message, MAX_DATA_SIZE};

// when the buffer is full, the server is expected to drop messages
// allowing the client to re-transmit at a later time (no ack is sent)
const CONNECTION_INCOMING_BUFFER_SIZE: usize = 128;

const INTERNAL_STREAM_SIZE: usize = 8184;
const INTERNAL_BUFFER_SIZE: usize = 128;

#[derive(Debug)]
enum InternalMessage {
    Ack { len: u32 },
    Data { position: u32, text: String },
}

#[derive(Debug, Clone)]
struct Connection {
    socket: Arc<UdpSocket>,
    addr: SocketAddr,
    session: u32,
    sent_len: Arc<Mutex<u32>>,
}

pub(super) fn spawn(
    socket: Arc<UdpSocket>,
    addr: SocketAddr,
    session: u32,
) -> (Handler, DuplexStream) {
    let (tx, from_listener) = mpsc::channel(CONNECTION_INCOMING_BUFFER_SIZE);
    let listener_handler = Handler { sender: tx, addr };

    let (handler_stream, conn_stream) = tokio::io::duplex(INTERNAL_STREAM_SIZE);

    let (send_data_from_client, receive_data_from_client) = mpsc::channel(1);
    let (send_data_to_client, receive_data_to_client) = mpsc::channel(INTERNAL_BUFFER_SIZE);
    let (send_ack, receive_ack) = mpsc::unbounded_channel();

    let connection = Connection {
        socket,
        addr,
        session,
        sent_len: Arc::new(Mutex::new(0)),
    };
    tokio::spawn(async move {
        tokio::select! {
            _ = listen_to_server(connection.clone(), from_listener, send_data_to_client, send_ack) => {},
            _ = listen_to_client(conn_stream, send_data_from_client, receive_data_to_client) => {},
            _ = data_sender(connection.clone(), receive_data_from_client, receive_ack) => {},
        };

        let _ = connection
            .socket
            .send_to(Message::close(session).to_string().as_bytes(), addr)
            .await;
    });

    (listener_handler, handler_stream)
}

async fn listen_to_server(
    connection: Connection,
    mut from_server: mpsc::Receiver<InternalMessage>,
    data_to_client: mpsc::Sender<String>,
    send_ack: mpsc::UnboundedSender<u32>,
) -> anyhow::Result<()> {
    let mut ack = 0;
    while let Some(message) = from_server.recv().await {
        match message {
            InternalMessage::Ack { len } => {
                if len > *connection.sent_len.lock().await {
                    // the client is misbehaving, terminate the connection
                    return Ok(());
                }

                send_ack
                    .send(len)
                    .context("the ack channel should live as long as the connection is open")?;
            }
            InternalMessage::Data { position, text } => {
                // if we didn't miss anything
                if position <= ack {
                    let old_data = (ack - position) as usize;
                    let mut rcount = 0;

                    if old_data < text.len() {
                        let relevant_data = &text[old_data..];
                        rcount = relevant_data.len();

                        match data_to_client.try_send(relevant_data.to_string()) {
                            Ok(_) => {
                                // data was sent succesfully
                            }
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                // internal buffer is full, simply ignore this message
                                // and let the client re-transmit it again, when hopefully
                                // some space in the buffer will be freed
                                continue;
                            }
                            // client was terminated
                            Err(mpsc::error::TrySendError::Closed(_)) => return Ok(()),
                        }
                    }

                    ack += rcount as u32;
                }

                // send an ack of what we've received so far
                connection
                    .socket
                    .send_to(
                        Message::ack(connection.session, ack).to_string().as_bytes(),
                        connection.addr,
                    )
                    .await?;
            }
        }
    }

    // the server has closed the connection
    Ok(())
}

async fn listen_to_client(
    stream: DuplexStream,
    data_from_client: mpsc::Sender<String>,
    mut data_to_client: mpsc::Receiver<String>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);

    let map_reader_to_sender_fut = async move {
        loop {
            let mut block = [0u8; MAX_DATA_SIZE];
            let rcount = reader.read(&mut block).await?;
            if rcount == 0 {
                break; // reached eof
            }

            data_from_client
                .send(
                    String::from_utf8(block[0..rcount].into())
                        .context("internal data should be a valid string")?,
                )
                .await?;
        }

        Ok::<(), anyhow::Error>(())
    };

    let map_receiver_to_writer_fut = async move {
        while let Some(data) = data_to_client.recv().await {
            writer.write_all(data.as_bytes()).await?;
        }

        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        result = map_reader_to_sender_fut => result,
        result = map_receiver_to_writer_fut => result,
    }
}

async fn data_sender(
    connection: Connection,
    mut receive_data: mpsc::Receiver<String>,
    mut receive_ack: mpsc::UnboundedReceiver<u32>,
) -> anyhow::Result<()> {
    let mut position: u32 = 0;
    let mut ack: u32 = 0;

    while let Some(data) = receive_data.recv().await {
        // local position for this transmission
        let mut sent_so_far: u32 = 0;

        while (sent_so_far as usize) < data.len() {
            let message = Message::data(
                connection.session,
                position + sent_so_far,
                data[sent_so_far as usize..].into(),
            )
            .to_string();
            let message = message.as_bytes();

            // wait for an ack
            let mut retry_interval = tokio::time::interval(RETRANSMISSION_TIMEOUT);
            let mut session_expiry_interval = tokio::time::interval(SESSION_EXPIRY_TIMEOUT);
            session_expiry_interval.tick().await; // first tick always return immediately

            loop {
                tokio::select! {
                    _ = retry_interval.tick() => {
                        let sent_len = &mut *connection.sent_len.lock().await;
                        connection.socket.send_to(message, connection.addr).await?;
                        *sent_len = position + data.len() as u32;
                    }
                    // client has disconnected
                    _ = session_expiry_interval.tick() => return Ok(()),
                    Some(ack_len) = receive_ack.recv() => {
                        if ack_len <= ack {
                            continue;
                        }

                        if ack_len as usize > (position as usize + data.len()) {
                            // client is misbehaving
                            return Ok(());
                        }

                        ack = ack_len;
                        sent_so_far = ack_len - position;
                        break;
                    },
                };
            }
        }

        position += sent_so_far;
    }

    // the client handler was dropped
    // terminate the connection

    Ok(())
}

pub(super) struct BufferIsFull;

// Handler for the listener to send incoming messages
pub(super) struct Handler {
    sender: mpsc::Sender<InternalMessage>,
    addr: SocketAddr,
}

impl Handler {
    pub(super) fn ack(&mut self, len: u32) -> Result<(), BufferIsFull> {
        self.sender
            .try_send(InternalMessage::Ack { len })
            .map_err(|_| BufferIsFull)
    }

    pub(super) fn data(&mut self, position: u32, text: String) -> Result<(), BufferIsFull> {
        self.sender
            .try_send(InternalMessage::Data { position, text })
            .map_err(|_| BufferIsFull)
    }

    pub(super) fn addr(&self) -> SocketAddr {
        self.addr
    }
}
