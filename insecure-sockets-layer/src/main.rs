use anyhow::Context;
use blueprint::Toy;
use protocol::connection::Connection;
use tokio::net::{TcpListener, TcpStream};

mod blueprint;
mod protocol;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // connect tracing to stdout
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    println!("Server listening on: {}", listener.local_addr().unwrap());

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_connection(conn));
    }
}

async fn handle_connection(conn: TcpStream) -> anyhow::Result<()> {
    let mut conn = Connection::new(conn).await?;
    tracing::debug!("sucessfully exchanged cipher spec, and initialized connection");

    while let Some(line) = conn.read_until(b'\n').await? {
        let line = String::from_utf8(line).context("data is assumed to be utf-8 encoded")?;
        tracing::debug!("received line: {}", line);

        let toys = line
            .split(',')
            .map(|toy| toy.parse::<Toy>())
            .collect::<Result<Vec<_>, _>>()
            .context("expected a list of toys")?;

        let most_important = toys
            .iter()
            .max()
            .context("expected at least 1 toy in the list")?;

        tracing::debug!("returned toy: {:?}", most_important);
        conn.write_all((most_important.to_string() + "\n").into())
            .await?;
    }

    Ok(())
}
