use async_tempfile::TempFile;
use sha1::{Digest, Sha1};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
};

use crate::{protocol::message, storage::ListResult};

use super::message::{Request, Response};

const BLOCK_SIZE: usize = 4096;

const READY_MSG: &[u8] = "READY\n".as_bytes();

pub struct Connection {
    stream: BufReader<TcpStream>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectionErr {
    #[error("Received an unknown method: {0}")]
    UnknownMethod(String),

    #[error("{0}")]
    Io(#[from] tokio::io::Error),

    #[error("{0}")]
    TempFile(#[from] async_tempfile::Error),

    #[error("Reached eof")]
    Eof,
}

impl Connection {
    /// Creates a new connection out of a TcpStream
    ///
    /// notifies the client that the server is ready on creation.
    pub async fn new(mut stream: TcpStream) -> tokio::io::Result<Self> {
        stream.write_all(READY_MSG).await?;
        tracing::debug!("a new connection has been initialized!");

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    /// Read a single request from the connection
    ///
    /// will continously read requests until it:
    /// - it receives an unknown method and return an error
    /// - receives a properly formated message, and returns it
    /// - reaches EOF, and returns None
    pub async fn read_request(&mut self) -> Result<Option<Request>, ConnectionErr> {
        // read and process raw requests until you reach a properly formatted request / reach EOF
        loop {
            let Some(request) = self.read_raw_request().await? else {
                return Ok(None);
            };

            match self.process_raw_request(request).await? {
                Ok(request) => return Ok(Some(request)),
                Err(response) => self.send_response(response).await?,
            }
        }
    }

    // Processes a raw request in an attemp to convert it to request
    // this function distinguishes between 2 types of errors:
    // - an error that can't be recovered and results in session termination
    // - an error that can be ignored, and we only need to notify the client
    async fn process_raw_request(
        &mut self,
        request: message::raw::Request,
    ) -> Result<Result<Request, Response>, ConnectionErr> {
        let request = match request {
            message::raw::Request::Help => Request::Help,
            message::raw::Request::List { path } => Request::List { path },
            message::raw::Request::Get { filename, revision } => {
                Request::Get { filename, revision }
            }
            message::raw::Request::Put {
                filename,
                byte_count,
            } => {
                // create a tempfile and attemp the read the requested number of bytes from the socket
                let mut file = TempFile::new().await?;

                // use this opportunity to also calculate the hash
                // of the file to avoid re-reading the file down the line
                let mut hasher = Sha1::new();

                // avoid creating a block that is bigger than the file itself
                let mut block = vec![0u8; BLOCK_SIZE.min(byte_count as usize)];
                let mut wcount = 0usize;
                loop {
                    // we've read the entire file
                    if (byte_count as usize) <= wcount {
                        break;
                    }

                    // block is too big, we must resize it to avoid over-reading
                    let remain = (byte_count as usize) - wcount;
                    if block.len() > remain {
                        block.resize(remain, 0)
                    }

                    let rcount = self.stream.read(&mut block).await?;
                    if rcount == 0 {
                        break;
                    }

                    if block[..rcount].iter().any(|byte| {
                        !byte.is_ascii_graphic()
                            && *byte != b'\r'
                            && *byte != b'\n'
                            && *byte != b' '
                            && *byte != b'\t'
                    }) {
                        return Ok(Err(Response::error("text files only".into())));
                    }

                    hasher.update(&block[..rcount]);
                    file.write_all(&block[..rcount]).await?;
                    wcount += rcount;
                }

                if wcount < byte_count as usize {
                    // reached EOF before reading the entirety of the file
                    return Err(ConnectionErr::Eof);
                }

                Request::Put {
                    filename,
                    file,
                    hash: hasher.finalize().to_vec(),
                }
            }
        };

        Ok(Ok(request))
    }

    // same as read_request, but for raw request
    async fn read_raw_request(&mut self) -> Result<Option<message::raw::Request>, ConnectionErr> {
        use message::raw::{Request, RequestErr};

        loop {
            // read new line
            let mut line = String::new();
            let rcount = self.stream.read_line(&mut line).await?;
            if rcount == 0 {
                return Ok(None);
            }

            tracing::debug!("received raw: {}", line);

            if line.trim().is_empty() {
                // skip empty lines
                continue;
            }

            // parse raw request
            match line.parse::<Request>() {
                Ok(request) => return Ok(Some(request)),
                Err(err) => {
                    // report error to client
                    self.send_response(Response::error(err.to_string())).await?;

                    if let RequestErr::IllegalMethod(method) = err {
                        tracing::debug!("received an illegal method \"{}\"", method);
                        return Err(ConnectionErr::UnknownMethod(method));
                    }

                    // received an acceptable error, continue to the next request
                    tracing::debug!("received a badly formated request: {}", err.to_string());
                    continue;
                }
            }
        }
    }

    /// Writes the given response to the client
    pub async fn send_response(&mut self, response: Response) -> Result<(), ConnectionErr> {
        use message::raw::Response;
        match response.raw {
            Response::Err(reason) => {
                self.stream
                    .write_all(format!("ERR {}\n", reason).as_bytes())
                    .await?
            }
            Response::Help => {
                self.stream
                    .write_all("OK usage: HELP|GET|PUT|LIST\n".as_bytes())
                    .await?
            }
            Response::Get { mut file } => {
                // make sure to read the file from the beginning
                file.seek(std::io::SeekFrom::Start(0)).await?;
                let metadata = file.metadata().await?;

                // use a buffer to avoid too many underlying syscalls
                let mut reader = BufReader::new(file);
                let mut writer = BufWriter::new(&mut self.stream);

                // write an OK status with file size information
                writer
                    .write_all(format!("OK {}\n", metadata.len()).as_bytes())
                    .await?;

                // dump the into the stream, in blocks
                // avoid creating a block with a size bigger than the file itself
                let mut block = vec![0u8; BLOCK_SIZE.min(metadata.len() as usize)];
                loop {
                    let rcount = reader.read(&mut block).await?;
                    if rcount == 0 {
                        // reached EOF
                        break;
                    }

                    writer.write_all(&block[..rcount]).await?;
                }

                // make sure to clean the buffer before we drop it
                writer.flush().await?;
            }
            Response::Put { revision } => {
                self.stream
                    .write_all(format!("OK r{}\n", revision).as_bytes())
                    .await?
            }
            Response::List { children } => {
                // use a buffer to avoid too many syscalls
                let mut writer = BufWriter::new(&mut self.stream);

                // write an OK status with the number of children
                writer
                    .write_all(format!("OK {}\n", children.len()).as_bytes())
                    .await?;

                // list the children
                for child in children {
                    match child {
                        ListResult::Dir(name) => {
                            writer
                                .write_all(format!("{}/ DIR\n", name).as_bytes())
                                .await?
                        }
                        ListResult::File {
                            name,
                            last_revision,
                        } => {
                            writer
                                .write_all(format!("{} r{}\n", name, last_revision).as_bytes())
                                .await?
                        }
                    }
                }

                writer.flush().await?;
            }
        };

        self.stream.write_all(READY_MSG).await?;

        Ok(())
    }
}
