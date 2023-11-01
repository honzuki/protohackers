use std::{
    collections::{hash_map, HashMap},
    net::SocketAddr,
    sync::Arc,
};

use tokio::{
    io::DuplexStream,
    net::{ToSocketAddrs, UdpSocket},
    sync::mpsc,
};

use super::{
    connection::{self, Handler},
    message::{Message, MessageType},
    MAX_MESSAGE_SIZE,
};

pub struct Listener {
    connections: mpsc::UnboundedReceiver<DuplexStream>,
    local_addr: SocketAddr,
}

impl Listener {
    // accept a new connection
    pub async fn accept(&mut self) -> tokio::io::Result<DuplexStream> {
        self.connections.recv().await.ok_or_else(|| {
            tokio::io::Error::new(
                tokio::io::ErrorKind::ConnectionAborted,
                "the connection channel has been closed",
            )
        })
    }

    // Bind a new listener to an address
    pub async fn bind<A>(addr: A) -> tokio::io::Result<Self>
    where
        A: ToSocketAddrs,
    {
        // use unbounded channel in order to never block the background task in charge of new connections.
        let (send_to_listener, rx) = mpsc::unbounded_channel();
        let socket = Arc::new(UdpSocket::bind(addr).await?);
        let local_addr = socket.local_addr()?;

        tokio::spawn(async move {
            let mut sessions: HashMap<u32, Handler> = HashMap::default();

            // for every new packet
            let mut packet = [0; MAX_MESSAGE_SIZE];
            while !send_to_listener.is_closed() || !sessions.is_empty() {
                let (len, addr) = socket.recv_from(&mut packet).await?;

                // parse the packet
                let Some(message) = dbg!(String::from_utf8(packet[..len].into())
                    .ok()
                    .and_then(|message| message.parse::<Message>().ok()))
                else {
                    continue; // badly formated message, ignore it
                };

                match message.ty {
                    MessageType::Connect => {
                        if let hash_map::Entry::Vacant(entry) = sessions.entry(message.session) {
                            if send_to_listener.is_closed() {
                                // listener was dropped - early exit
                                continue;
                            }

                            let (handler, conn) =
                                connection::spawn(socket.clone(), addr, message.session);
                            if send_to_listener.send(conn).is_err() {
                                // listener was dropped
                                continue;
                            }
                            entry.insert(handler);
                        }

                        socket
                            .send_to(
                                Message::ack(message.session, 0).to_string().as_bytes(),
                                addr,
                            )
                            .await?;
                    }
                    MessageType::Close => {
                        if let Some(conn) = sessions.get(&message.session) {
                            if addr == conn.addr() {
                                // make sure the client owns the session
                                sessions.remove(&message.session);
                            }
                        }

                        // either way send a close message
                        socket
                            .send_to(Message::close(message.session).to_string().as_bytes(), addr)
                            .await?;
                    }
                    MessageType::Ack { length } => {
                        // reject unknown sessions with a close message
                        let Some(conn) = sessions.get_mut(&message.session) else {
                            socket
                                .send_to(
                                    Message::close(message.session).to_string().as_bytes(),
                                    addr,
                                )
                                .await?;
                            continue;
                        };

                        // if the buffer is full, allow the client retransmit the ack
                        let _ = conn.ack(length);
                    }
                    MessageType::Data { position, data } => {
                        // reject unknown sessions with a close message
                        let Some(conn) = sessions.get_mut(&message.session) else {
                            socket
                                .send_to(
                                    Message::close(message.session).to_string().as_bytes(),
                                    addr,
                                )
                                .await?;
                            continue;
                        };

                        // if the buffer is full, allow the client retransmit the data
                        let _ = conn.data(position, data);
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        });

        Ok(Self {
            connections: rx,
            local_addr,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}
