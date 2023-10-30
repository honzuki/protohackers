use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use super::message::{message_type, ToClient, ToClientInternal};

#[async_trait]
pub trait Serialize: Sized {
    type Error;

    /// Serialize a structure into a writer
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(
        &self,
        writer: &mut W,
    ) -> Result<(), Self::Error>;
}

#[derive(thiserror::Error, Debug)]
pub enum SerializeError {
    #[error("The input is too long!")]
    TooLong,

    #[error("{0}")]
    Io(#[from] tokio::io::Error),
}

#[async_trait]
impl Serialize for &str {
    type Error = SerializeError;

    async fn serialize<W: AsyncWriteExt + Unpin + Send>(
        &self,
        writer: &mut W,
    ) -> Result<(), Self::Error> {
        let length: u8 = self.len().try_into().map_err(|_| SerializeError::TooLong)?;

        writer.write_u8(length).await?;
        writer.write_all(self.as_bytes()).await?;

        Ok(())
    }
}

#[async_trait]
impl Serialize for ToClient {
    type Error = SerializeError;

    async fn serialize<W: AsyncWriteExt + Unpin + Send>(
        &self,
        writer: &mut W,
    ) -> Result<(), Self::Error> {
        match &self.internal {
            ToClientInternal::Heartbeat => writer.write_u8(message_type::HEARTBEAT).await?,
            ToClientInternal::Error { msg } => {
                writer.write_u8(message_type::ERROR).await?;
                msg.as_str().serialize(writer).await?;
            }
            ToClientInternal::Ticket {
                plate,
                road,
                first_record,
                second_record,
                speed,
            } => {
                writer.write_u8(message_type::TICKET).await?;
                plate.as_str().serialize(writer).await?;
                writer.write_u16(*road).await?;
                writer.write_u16(first_record.0).await?;
                writer.write_u32(first_record.1).await?;
                writer.write_u16(second_record.0).await?;
                writer.write_u32(second_record.1).await?;
                writer.write_u16(*speed).await?;
            }
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{
        message::{ToClient, ToClientInternal},
        serializer::Serialize,
    };

    #[tokio::test]
    async fn serialize_basic_types() {
        let text = "check proper string serialization";
        let mut serialized_text = vec![];
        text.serialize(&mut serialized_text).await.unwrap();
        let expected_text = b"\x21\x63\x68\x65\x63\x6b\x20\x70\x72\x6f\x70\x65\x72\x20\x73\x74\x72\x69\x6e\x67\x20\x73\x65\x72\x69\x61\x6c\x69\x7a\x61\x74\x69\x6f\x6e";
        assert_eq!(serialized_text, expected_text);
    }

    #[tokio::test]
    async fn serialize_messages() {
        let values = [
            ToClient {
                internal: ToClientInternal::Error { msg: "bad".into() },
            },
            ToClient {
                internal: ToClientInternal::Error {
                    msg: "illegal msg".into(),
                },
            },
            ToClient {
                internal: ToClientInternal::Ticket {
                    plate: "UN1X".into(),
                    road: 66,
                    first_record: (100, 123456),
                    second_record: (110, 123816),
                    speed: 10000,
                },
            },
            ToClient {
                internal: ToClientInternal::Ticket {
                    plate: "RE05BKG".into(),
                    road: 368,
                    first_record: (1234, 1000000),
                    second_record: (1235, 1000060),
                    speed: 6000,
                },
            },
            ToClient {
                internal: ToClientInternal::Heartbeat,
            },
        ];

        let mut serialized_values = Vec::with_capacity(values.len());
        for value in values {
            let mut raw = vec![];
            value.serialize(&mut raw).await.unwrap();
            serialized_values.push(raw);
        }

        let expected_values: [&[u8]; 5] = [
            b"\x10\x03\x62\x61\x64",
            b"\x10\x0b\x69\x6c\x6c\x65\x67\x61\x6c\x20\x6d\x73\x67",
            b"\x21\x04\x55\x4e\x31\x58\x00\x42\x00\x64\x00\x01\xe2\x40\x00\x6e\x00\x01\xe3\xa8\x27\x10",
            b"\x21\x07\x52\x45\x30\x35\x42\x4b\x47\x01\x70\x04\xd2\x00\x0f\x42\x40\x04\xd3\x00\x0f\x42\x7c\x17\x70",
            b"\x41"
        ];

        assert_eq!(serialized_values, expected_values)
    }
}
