use protocol::{
    connection::Connection,
    message::{Request, Response},
};
use storage::TempFileSystem;
use tokio::net::{TcpListener, TcpStream};

mod protocol;
mod storage;

type SharedFileSystem = &'static TempFileSystem;

#[tokio::main]
async fn main() -> tokio::io::Result<()> {
    tracing_subscriber::fmt::init();

    let shared_filesystem = Box::leak(Box::default());

    let listener = TcpListener::bind("0.0.0.0:3600").await?;
    tracing::info!("server is listening on: {}", listener.local_addr()?);

    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_connection(conn, shared_filesystem));
    }
}

async fn handle_connection(stream: TcpStream, fs: SharedFileSystem) -> anyhow::Result<()> {
    let mut client = Connection::new(stream).await?;

    while let Some(request) = client.read_request().await? {
        tracing::debug!("received request: {:?}", request);

        let response = match request {
            Request::Put {
                filename,
                file,
                hash,
            } => {
                let revision = fs.insert(filename, file, hash);
                Response::put(revision)
            }
            Request::Get { filename, revision } => match fs.get(&filename, revision).await {
                Ok(file) => Response::get(file),
                Err(reason) => Response::error(reason.to_string()),
            },
            Request::List { path } => {
                let children = fs.list(&path);
                Response::list(children)
            }
            Request::Help => Response::help(),
        };

        tracing::debug!("responded: {:?}", response);
        client.send_response(response).await?;
    }

    Ok(())
}
