use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use super::{
    cipher::{self, CipherParseErr},
    MAX_CIPHER_SPEC_LEN, MAX_LINE_LEN,
};

/// A useful wrapper that takes care of
/// encrypting/decrypting all data from/to the server
pub struct Connection {
    buffer: BytesMut,
    stream: TcpStream,
    cipher: cipher::Spec,
    decrypt_position: usize,
    encrypt_position: usize,
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectionErr {
    #[error("{0}")]
    Io(#[from] tokio::io::Error),

    #[error("{0}")]
    CipherParse(#[from] CipherParseErr),

    #[error("No cipher was provided")]
    MissingCipher,

    #[error("The cipher spec is too long")]
    CipherIsTooLong,

    #[error("The cipher must not be equal to a no-op")]
    NoOpCipher,

    #[error("The block is too long")]
    BlockIsTooLong,
}

impl Connection {
    pub async fn new(stream: TcpStream) -> Result<Self, ConnectionErr> {
        let mut buffer = BytesMut::new();
        let mut stream = stream;

        let cipher = read_cipher(&mut stream, &mut buffer).await?;
        tracing::debug!("received cipher spec: {:?}", cipher);
        if cipher.is_noop() {
            tracing::debug!("cipher spec is equal to no-op: {:?}", cipher);
            return Err(ConnectionErr::NoOpCipher);
        }

        // decrypt the remianing data in the buffer
        cipher.decrypt(&mut buffer, 0);

        Ok(Self {
            decrypt_position: buffer.len(),
            buffer,
            stream,
            cipher,
            encrypt_position: 0,
        })
    }

    /// reads a block of data from the stream until it receives 'expected_byte',
    /// and returns the entire block, excluding the expected_byte at the end.
    ///
    /// returns an error if it reaches EOF in the _middle of a block_, but otherwise None.
    pub async fn read_until(
        &mut self,
        expected_byte: u8,
    ) -> Result<Option<Vec<u8>>, ConnectionErr> {
        let mut position = 0;

        loop {
            // check if we found the 'expetced_byte'
            for idx in position..self.buffer.len() {
                if self.buffer[idx] == expected_byte {
                    // found the end of the block,
                    // remove it from the buffer and return to the user
                    let block = self.buffer[..idx].to_vec();
                    self.buffer.advance(idx + 1);
                    return Ok(Some(block));
                }
            }

            // set position to the last byte we didn't check yet
            position = self.buffer.len();
            if position > MAX_LINE_LEN {
                // allow a little more than the max
                return Err(ConnectionErr::BlockIsTooLong);
            }

            // read some new data into the buffer
            let rcount = self.stream.read_buf(&mut self.buffer).await?;
            if rcount == 0 {
                if position == 0 {
                    // reached EOF before reading anything
                    return Ok(None);
                }

                // reaches EOF in the middle of a block
                return Err(tokio::io::Error::new(
                    tokio::io::ErrorKind::UnexpectedEof,
                    "reached EOF in the of reading a block",
                )
                .into());
            }

            // decrypt the new data in the buffer
            self.cipher
                .decrypt(&mut self.buffer[position..], self.decrypt_position);
            self.decrypt_position += rcount;
        }
    }

    /// dumps data into the stream
    pub async fn write_all(&mut self, mut data: Vec<u8>) -> tokio::io::Result<()> {
        self.cipher.encrypt(&mut data, self.encrypt_position);
        self.encrypt_position += data.len();

        self.stream.write_all(&data).await
    }
}

async fn read_cipher(
    stream: &mut TcpStream,
    buffer: &mut BytesMut,
) -> Result<cipher::Spec, ConnectionErr> {
    // read the cipher spec
    let mut position = 0;
    while position < MAX_CIPHER_SPEC_LEN {
        // read some new data into the buffer
        let rcount = stream.read_buf(buffer).await?;
        if rcount == 0 {
            // reached EOF before reading a cipher
            return Err(ConnectionErr::MissingCipher);
        }

        // for every new byte in the buffer
        let end_idx = buffer.len().min(MAX_CIPHER_SPEC_LEN);
        for idx in position..end_idx {
            if buffer[idx] == 0 {
                // read a cipher into buffer, try to parse and return it
                let spec: cipher::Spec = buffer[0..idx].try_into()?;
                // make sure to discard the cipher spec from the buffer
                buffer.advance(idx + 1);

                return Ok(spec);
            }
        }
        position = end_idx;
    }

    Err(ConnectionErr::CipherIsTooLong)
}
