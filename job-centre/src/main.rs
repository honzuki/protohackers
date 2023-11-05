use std::sync::{Arc, Mutex};

use client::Client;
use jobs::Manager;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

mod client;
mod jobs;
mod request;

type SharedJobManager = Arc<Mutex<Manager>>;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    // connect tracing to stdout
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    tracing::info!("Server listening on: {}", listener.local_addr()?);

    let shared_job_manager = SharedJobManager::default();

    loop {
        let (conn, _) = listener.accept().await?;
        let client = Client::new(shared_job_manager.clone());
        tokio::spawn(handle_request(client, conn));
    }
}

async fn handle_request(mut client: Client, mut stream: TcpStream) -> tokio::io::Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    loop {
        let mut request = String::new();
        let rcount = reader.read_line(&mut request).await?;
        if rcount == 0 {
            break; // EOF
        }

        tracing::debug!("received: {}", request);
        let response = client.handle_request(&request).await;
        tracing::debug!("responded: {:?}", response);

        if let Ok(mut response) = serde_json::to_string(&response) {
            response.push('\n');
            writer.write_all(response.as_bytes()).await?;
        }
    }

    Ok(())
}
