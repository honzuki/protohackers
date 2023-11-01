use std::{num::ParseIntError, str::FromStr};

#[derive(Debug, PartialEq)]
pub struct Message {
    pub session: u32,
    pub ty: MessageType,
}

impl Message {
    pub fn data(session: u32, position: u32, data: String) -> Self {
        Self {
            session,
            ty: MessageType::Data { position, data },
        }
    }

    pub fn ack(session: u32, length: u32) -> Self {
        Self {
            session,
            ty: MessageType::Ack { length },
        }
    }

    pub fn close(session: u32) -> Self {
        Self {
            session,
            ty: MessageType::Close,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MessageType {
    Connect,
    Data { position: u32, data: String },
    Ack { length: u32 },
    Close,
}

impl ToString for Message {
    fn to_string(&self) -> String {
        let session = self.session.to_string();
        let session = session.as_str();

        let body = match &self.ty {
            MessageType::Connect => "connect".to_string() + "/" + session,
            MessageType::Close => "close".to_string() + "/" + session,
            MessageType::Ack { length } => {
                "ack".to_string() + "/" + session + "/" + length.to_string().as_str()
            }
            MessageType::Data { position, data } => {
                "data".to_string()
                    + "/"
                    + session
                    + "/"
                    + position.to_string().as_str()
                    + "/"
                    // escape slashes
                    + data.replace('\\', r"\\").replace('/', r"\/").as_str()
            }
        };

        // wrap body inside two '/'
        "/".to_string() + &body + "/"
    }
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum ParseMessageError {
    #[error("unknown message format")]
    Unknown,

    #[error("{0}")]
    ParseInt(#[from] ParseIntError),

    #[error("the data part wasn't escaped properly")]
    BadDataFormat,
}

impl FromStr for Message {
    type Err = ParseMessageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() < 2 || !s.starts_with('/') || !s.ends_with('/') {
            return Err(ParseMessageError::Unknown);
        }

        // remove the wrapping '/' and split over all parts (ignore escaping problems for now)
        let mut parts = s[1..s.len() - 1].split('/');
        let ty = parts.next().ok_or(ParseMessageError::Unknown)?;
        let session: u32 = parts.next().ok_or(ParseMessageError::Unknown)?.parse()?;

        let message = match ty {
            "connect" => {
                if parts.next().is_some() {
                    return Err(ParseMessageError::Unknown);
                }

                Self {
                    session,
                    ty: MessageType::Connect,
                }
            }
            "close" => {
                if parts.next().is_some() {
                    return Err(ParseMessageError::Unknown);
                }

                Self {
                    session,
                    ty: MessageType::Close,
                }
            }
            "ack" => {
                let length: u32 = parts.next().ok_or(ParseMessageError::Unknown)?.parse()?;
                if parts.next().is_some() {
                    return Err(ParseMessageError::Unknown);
                }

                Self {
                    session,
                    ty: MessageType::Ack { length },
                }
            }
            "data" => {
                let position: u32 = parts.next().ok_or(ParseMessageError::Unknown)?.parse()?;
                let data = parts.collect::<Vec<_>>().join("/");

                Self {
                    session,
                    ty: MessageType::Data {
                        position,
                        data: unescape_data(&data)?,
                    },
                }
            }
            _ => return Err(ParseMessageError::Unknown),
        };

        Ok(message)
    }
}

fn unescape_data(data: &str) -> Result<String, ParseMessageError> {
    // make sure the data is properly formated:
    // every '\' follows either '\' or '/'
    // no '/' or '\' appears without a '\' before it
    let mut chars = data.chars();

    let mut last = chars.next();
    while let Some(ch) = chars.next() {
        if matches!(last, Some('\\')) {
            if ch != '/' && ch != '\\' {
                return Err(ParseMessageError::BadDataFormat);
            }

            last = chars.next();
            continue;
        }

        if ch == '/' {
            return Err(ParseMessageError::BadDataFormat);
        }

        last = Some(ch);
    }

    if matches!(last, Some('\\') | Some('/')) {
        return Err(ParseMessageError::BadDataFormat);
    }

    Ok(data.replace(r"\\", r"\").replace(r"\/", "/"))
}

#[cfg(test)]
mod tests {
    use super::{Message, MessageType};

    #[test]
    fn deserialize_properly_formated_messages() {
        let raw_messages = [
            r"/data/1234567/0/hello/",
            r"/connect/1234567/",
            r"/ack/1234567/5/",
            r"/data/1234568/0/\//",
            r"/close/1234567/",
            r"/data/12345/50/Hello, world!/",
            r"/data/510246063/0/a\//",
        ];
        let expected_messages = [
            Message {
                session: 1234567,
                ty: MessageType::Data {
                    position: 0,
                    data: "hello".into(),
                },
            },
            Message {
                session: 1234567,
                ty: MessageType::Connect,
            },
            Message {
                session: 1234567,
                ty: MessageType::Ack { length: 5 },
            },
            Message {
                session: 1234568,
                ty: MessageType::Data {
                    position: 0,
                    data: "/".into(),
                },
            },
            Message {
                session: 1234567,
                ty: MessageType::Close,
            },
            Message {
                session: 12345,
                ty: MessageType::Data {
                    position: 50,
                    data: "Hello, world!".into(),
                },
            },
            Message {
                session: 510246063,
                ty: MessageType::Data {
                    position: 0,
                    data: r"a/".into(),
                },
            },
        ];

        for (raw, expected) in raw_messages.iter().zip(expected_messages) {
            assert_eq!(raw.parse::<Message>(), Ok(expected))
        }
    }

    #[test]
    fn deserialize_bad_formated_messages() {
        let raw_messages = [
            r"/data/12345/999999999999999999999/the number should be too long!/",
            r"/data/231/1/Hello!",
            r"ack/123/1/",
            r"/data/3/1/hel\lo/world/",
            r"/data/4/5/hello\///",
            r"/data/6/7/\/",
            r"/data/6/7///",
        ];

        for raw in raw_messages {
            let parsed = raw.parse::<Message>();
            if parsed.is_ok() {
                panic!("expected an error, received: {:?}", parsed);
            }
        }
    }

    #[test]
    fn check_serializer() {
        // we know the deserializer work properly, we can use it to verify the serializer
        let raw_messages = [
            r"/data/1234567/0/hello/",
            r"/connect/1234567/",
            r"/ack/1234567/5/",
            r"/data/1234568/0/\//",
            r"/close/1234567/",
            r"/data/12345/50/Hello, world!/",
        ];

        for raw in raw_messages {
            assert_eq!(raw.parse::<Message>().unwrap().to_string(), raw);
        }
    }
}
