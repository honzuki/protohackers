use async_trait::async_trait;
use tokio::io::AsyncReadExt;

use super::message::{message_type, FromClient};

#[async_trait]
pub trait Deserialize: Sized {
    type Error;

    // Deserialize a structure from a reader
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(
        reader: &mut R,
    ) -> Result<Self, Self::Error>;
}

#[derive(thiserror::Error, Debug)]
pub enum DeserializeError {
    #[error("{0}")]
    Utf(#[from] std::string::FromUtf8Error),

    #[error("{0}")]
    Io(#[from] tokio::io::Error),

    #[error("Unknown message type: {0}")]
    UnknownType(u8),
}

#[async_trait]
impl Deserialize for Vec<u16> {
    type Error = tokio::io::Error;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(
        reader: &mut R,
    ) -> Result<Self, Self::Error> {
        let length = reader.read_u8().await?;
        let mut data = Vec::with_capacity(length as usize);

        for _ in 0..length {
            data.push(reader.read_u16().await?);
        }

        Ok(data)
    }
}

#[async_trait]
impl Deserialize for String {
    type Error = DeserializeError;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(
        reader: &mut R,
    ) -> Result<Self, Self::Error> {
        // Read raw bytes
        let length = reader.read_u8().await?;
        let mut raw = vec![0u8; length as usize];
        reader.read_exact(&mut raw).await?;

        // Parse the raw bytes into a string
        let text = String::from_utf8(raw)?;

        Ok(text)
    }
}

#[async_trait]
impl Deserialize for FromClient {
    type Error = DeserializeError;

    async fn deserialize<R: AsyncReadExt + Unpin + Send>(
        reader: &mut R,
    ) -> Result<Self, Self::Error> {
        let ty = reader.read_u8().await?;

        let msg = match ty {
            message_type::PLATE => Self::Plate {
                plate: String::deserialize(reader).await?.trim().to_owned(),
                timestamp: reader.read_u32().await?,
            },
            message_type::WANT_HEARTBEAT => Self::WantHeartbeat {
                interval: reader.read_u32().await?,
            },
            message_type::I_AM_CAMERA => Self::IAmCamera {
                road: reader.read_u16().await?,
                mile: reader.read_u16().await?,
                limit: reader.read_u16().await?,
            },
            message_type::I_AM_DISPATCHER => Self::IAmDispatcher {
                roads: Vec::deserialize(reader).await?,
            },

            _ => return Err(DeserializeError::UnknownType(ty)),
        };

        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{deserializer::Deserialize, message::FromClient};

    #[tokio::test]
    async fn deserialize_basic_types() {
        let raw_text = b"\x23\x63\x68\x65\x63\x6b\x20\x70\x72\x6f\x70\x65\x72\x20\x73\x74\x72\x69\x6e\x67\x20\x64\x65\x73\x65\x72\x69\x61\x6c\x69\x7a\x61\x74\x69\x6f\x6e";
        let deserialized_text = String::deserialize(&mut raw_text.as_ref()).await.unwrap();
        let expected_text = "check proper string deserialization";
        assert_eq!(deserialized_text, expected_text);

        let raw_vec = b"\x03\x00\x42\x01\x70\x13\x88";
        let deserialized_vec: Vec<u16> = Vec::deserialize(&mut raw_vec.as_ref()).await.unwrap();
        let expected_vec = &[66u16, 368, 5000];
        assert_eq!(deserialized_vec, expected_vec);
    }

    #[tokio::test]
    async fn deserialize_messages() {
        let raw_values: [&[u8]; 8] = [
            b"\x20\x04\x55\x4E\x31\x58\x00\x00\x03\xE8",
            b"\x20\x07\x52\x45\x30\x35\x42\x4b\x47\x00\x01\xE2\x40",
            b"\x40\x00\x00\x00\x0a",
            b"\x40\x00\x00\x04\xdb",
            b"\x80\x00\x42\x00\x64\x00\x3c",
            b"\x80\x01\x70\x04\xd2\x00\x28",
            b"\x81\x01\x00\x42",
            b"\x81\x03\x00\x42\x01\x70\x13\x88",
        ];

        let mut deserialized_values = Vec::with_capacity(raw_values.len());
        for mut value in raw_values {
            deserialized_values.push(FromClient::deserialize(&mut value).await.unwrap());
        }

        let expected_values = [
            FromClient::Plate {
                plate: "UN1X".into(),
                timestamp: 1000,
            },
            FromClient::Plate {
                plate: "RE05BKG".into(),
                timestamp: 123456,
            },
            FromClient::WantHeartbeat { interval: 10 },
            FromClient::WantHeartbeat { interval: 1243 },
            FromClient::IAmCamera {
                road: 66,
                mile: 100,
                limit: 60,
            },
            FromClient::IAmCamera {
                road: 368,
                mile: 1234,
                limit: 40,
            },
            FromClient::IAmDispatcher { roads: [66].into() },
            FromClient::IAmDispatcher {
                roads: [66, 368, 5000].into(),
            },
        ];

        assert_eq!(deserialized_values, expected_values)
    }
}
