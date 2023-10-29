use std::{net::SocketAddr, sync::Arc};

use protocol::Request;
use tokio::net::UdpSocket;

mod db;
mod protocol;

struct SharedState {
    kv: db::KeyValue,
    socket: UdpSocket,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:3606").await?;
    println!("Server listening on: {}", socket.local_addr()?);

    let state = Arc::new(SharedState {
        kv: db::KeyValue::default(),
        socket,
    });

    let mut packet = [0; 1024];
    loop {
        let (len, addr) = state.socket.recv_from(&mut packet).await?;
        tokio::spawn(handle_request(state.clone(), addr, packet[..len].to_vec()));
    }
}

async fn handle_request(
    state: Arc<SharedState>,
    client: SocketAddr,
    packet: Vec<u8>,
) -> anyhow::Result<()> {
    let request = Request::from_string(String::from_utf8(packet)?);

    match request {
        Request::Insert(key, value) => state.kv.set(key, value),
        Request::Retrieve(key) => {
            if let Some(value) = state.kv.get(&key) {
                let response = key + "=" + &value;
                state.socket.send_to(response.as_bytes(), client).await?;
            }
        }
    }

    Ok(())
}
