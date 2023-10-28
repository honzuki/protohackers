use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::protocol::{MAX_MESSAGE_SIZE, MAX_USERNAME_SIZE, SYSTEM_MESSAGE_PREFIX};

pub struct Writer<W> {
    writer: W,
}

impl<W> Writer<W>
where
    W: Unpin,
    W: AsyncWrite,
{
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub async fn send_welcome_message(&mut self) -> tokio::io::Result<()>
    where
        Self: Unpin,
    {
        self.writer
            .write_all("Welcome to budgetchat! What shall I call you?\n".as_bytes())
            .await?;
        self.writer.flush().await?;

        Ok(())
    }

    pub async fn send_user_list(&mut self, userlist: Vec<String>) -> tokio::io::Result<()>
    where
        Self: Unpin,
    {
        self.writer
            .write_all(
                format!(
                    "{} The room contains: {}\n",
                    SYSTEM_MESSAGE_PREFIX,
                    userlist.join(",")
                )
                .as_bytes(),
            )
            .await?;
        self.writer.flush().await?;

        Ok(())
    }

    pub async fn send_message(&mut self, from: &str, message: &str) -> tokio::io::Result<()>
    where
        Self: Unpin,
    {
        self.writer
            .write_all(format!("[{}] {}\n", from, message).as_bytes())
            .await?;
        self.writer.flush().await?;

        Ok(())
    }

    pub async fn send_join_message(&mut self, username: &str) -> tokio::io::Result<()>
    where
        Self: Unpin,
    {
        self.writer
            .write_all(
                format!(
                    "{} {} has enetered the room\n",
                    SYSTEM_MESSAGE_PREFIX, username
                )
                .as_bytes(),
            )
            .await?;
        self.writer.flush().await?;

        Ok(())
    }

    pub async fn send_left_message(&mut self, username: &str) -> tokio::io::Result<()>
    where
        Self: Unpin,
    {
        self.writer
            .write_all(
                format!("{} {} has left the room\n", SYSTEM_MESSAGE_PREFIX, username).as_bytes(),
            )
            .await?;
        self.writer.flush().await?;

        Ok(())
    }
}

pub struct Reader<R> {
    reader: R,
}

#[derive(thiserror::Error, Debug)]
pub enum ReaderError {
    #[error("{0}")]
    Io(#[from] tokio::io::Error),

    #[error("Reached EOF")]
    Eof,

    #[error("Received a non ascii message")]
    NonAscii,

    #[error("Username must consist entirely of alphanumeric characteres, and contain at least one character")]
    InvalidUsername,
}

impl<R> Reader<R>
where
    R: Unpin,
    R: AsyncRead,
{
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub async fn read_name(&mut self) -> Result<String, ReaderError> {
        let name = self.read_limited_line(MAX_USERNAME_SIZE).await?;
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ReaderError::InvalidUsername);
        }

        Ok(name)
    }

    pub async fn read_message(&mut self) -> Result<String, ReaderError> {
        self.read_limited_line(MAX_MESSAGE_SIZE).await
    }

    async fn read_limited_line(&mut self, size: usize) -> Result<String, ReaderError> {
        // limit the reader
        let mut buf = BufReader::new(&mut self.reader).take(size as u64);

        // reader a line
        let mut content = String::with_capacity(size);
        let rcount = buf.read_line(&mut content).await?;
        if rcount == 0 {
            return Err(ReaderError::Eof);
        }

        // verify that the content is valid ASCII
        if !content.is_ascii() {
            return Err(ReaderError::NonAscii);
        }

        // remove new line from the end
        content.pop();
        println!("{}", content);
        Ok(content)
    }
}
