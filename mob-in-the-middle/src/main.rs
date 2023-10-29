use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};

mod proxy;

const BUDGET_CHAT_ADDR: &str = "chat.protohackers.com:16963";

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    println!("Server listening on: {}", listener.local_addr()?);

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_connection(conn));
    }
}

async fn handle_connection(mut client: TcpStream) -> tokio::io::Result<()> {
    let mut server = TcpStream::connect(BUDGET_CHAT_ADDR).await?;

    // Split the streams into reader/writer
    let (creader, cwriter) = client.split();
    let (sreader, swriter) = server.split();

    // connect creader with swriter & sreader with cwriter
    let client_to_server_proxy = connect_reader_to_writer(creader, swriter);
    let server_to_client_proxy = connect_reader_to_writer(sreader, cwriter);

    // wait until either of the ends terminate
    tokio::select! {
        _ = client_to_server_proxy => {}
        _ = server_to_client_proxy => {}
    };

    Ok(())
}

async fn connect_reader_to_writer<R, W>(reader: R, writer: W) -> tokio::io::Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut writer = proxy::Writer::new(writer);

    loop {
        let mut line = String::new();
        let rcount = reader.read_line(&mut line).await?;
        if rcount == 0 {
            break;
        }

        writer.write(&line).await?;
    }

    Ok(())
}
