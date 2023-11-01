use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

mod lrcp;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let mut listener = lrcp::Listener::bind("0.0.0.0:3600").await?;
    println!("listening on: {}", listener.local_addr());

    loop {
        let conn = listener.accept().await?;
        tokio::spawn(handle_connection(conn));
    }
}

async fn handle_connection(conn: DuplexStream) -> tokio::io::Result<()> {
    let (reader, mut writer) = tokio::io::split(conn);
    let mut reader = BufReader::new(reader);

    loop {
        let mut line = String::new();
        let rcount = reader.read_line(&mut line).await?;
        if rcount == 0 {
            break;
        }

        // remove the newline char
        line.pop();
        // reverse the line
        let mut reversed_line = line.chars().rev().collect::<String>();
        // add the new line back
        reversed_line.push('\n');

        // reverse the line and send it back
        writer.write_all(reversed_line.as_bytes()).await?;
    }

    Ok(())
}
