use protocol::{Request, Response};
use timetable::Table;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

mod protocol;
mod timetable;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_request(conn));
    }
}

async fn handle_request(mut client: TcpStream) {
    let mut table = Table::default();

    let mut frame = [0u8; 9];
    while let Ok(9) = client.read_exact(&mut frame).await {
        let request = Request::from_bytes(&frame).expect("received bad frame");
        match request {
            Request::Insert { timestamp, price } => {
                table.set_price(timestamp, price);
            }
            Request::Query { min_time, max_time } => {
                let avg = table.average(min_time, max_time);
                let response = Response::create_query_response(avg);
                client
                    .write_all(&response.to_bytes()[..])
                    .await
                    .expect("write to client");
            }
        }
    }
}
