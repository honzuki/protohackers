use tokio::net::TcpListener;

mod client;
mod protocol;
mod systems;

#[derive(Debug, Clone)]
pub struct SharedSystems {
    ticket: systems::ticket::Handler,
    record: systems::record::Handler,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ticket_system = systems::ticket::System::start();
    let record_system = systems::record::System::start(ticket_system.clone());

    let shared_systems = SharedSystems {
        ticket: ticket_system,
        record: record_system,
    };

    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    println!("Server listening on: {}", listener.local_addr().unwrap());

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(client::handle(conn, shared_systems.clone()));
    }
}
