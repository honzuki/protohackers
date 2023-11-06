//: A reverse proxy that is used to connect the tests with the trial-copy
//: in an attemp to understand the underlying protocol they use to communicate.

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{ReadHalf, WriteHalf},
        TcpListener, TcpStream,
    },
};

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:3600").await?;

    loop {
        let (mut client, _) = listener.accept().await?;

        tokio::spawn(async move {
            let (creader, cwriter) = client.split();
            let mut server = TcpStream::connect("vcs.protohackers.com:30307").await?;
            let (sreader, swriter) = server.split();

            let mut client_output = String::new();
            let mut server_output = String::new();

            // connect the client & the server
            let result = tokio::select! {
                result = connect_reader_writer(&mut server_output, sreader, cwriter) => result,
                result = connect_reader_writer(&mut client_output, creader, swriter) => result,
            };

            println!("From server:\n\n{}", server_output);
            println!("From client:\n\n{}", client_output);

            result
        });
    }
}

async fn connect_reader_writer(
    output: &mut String,
    mut reader: ReadHalf<'_>,
    mut writer: WriteHalf<'_>,
) -> tokio::io::Result<()> {
    let mut block = vec![0u8; 4098];
    loop {
        let rcount = reader.read(&mut block).await?;
        if rcount == 0 {
            break;
        }

        output.push_str(&String::from_utf8_lossy(&block[..rcount]));

        writer.write_all(&block[..rcount]).await?;
    }

    Ok(())
}
