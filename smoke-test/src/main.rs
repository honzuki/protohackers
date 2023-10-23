use std::io;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    loop {
        let (mut conn, _) = listener.accept().await?;
        tokio::spawn(async move {
            let (mut reader, mut writer) = TcpStream::split(&mut conn);
            let _ = tokio::io::copy(&mut reader, &mut writer).await;
        });
    }
}
