#[derive(thiserror::Error, Debug)]
pub enum RequestError {
    #[error("{0}")]
    IO(#[from] tokio::io::Error),
    #[error("Received an unknown type: {0:X}")]
    UnknownType(u8),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Insert { timestamp: i32, price: i32 },
    Query { min_time: i32, max_time: i32 },
}

impl Request {
    pub fn from_bytes(bytes: &[u8; 9]) -> Result<Self, RequestError> {
        let ty = bytes[0];
        let i1 = i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let i2 = i32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);

        match ty {
            b'I' => Ok(Request::Insert {
                timestamp: i1,
                price: i2,
            }),
            b'Q' => Ok(Request::Query {
                min_time: i1,
                max_time: i2,
            }),
            _ => Err(RequestError::UnknownType(bytes[0])),
        }
    }
}

#[derive(Debug)]
pub struct Response {
    average: i32,
}

impl Response {
    pub fn create_query_response(average: i32) -> Self {
        Self { average }
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        self.average.to_be_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn check_request_parsing() {
        let raw_requests = [
            b"\x49\x00\x00\xa0\x00\x00\x00\x00\x05",
            b"\x51\x00\x00\x30\x00\x00\x00\x40\x00",
        ];

        let expected_requests = [
            Request::Insert {
                timestamp: 40960,
                price: 5,
            },
            Request::Query {
                min_time: 12288,
                max_time: 16384,
            },
        ];

        for (raw, expected) in raw_requests.iter().zip(expected_requests) {
            assert_eq!(Request::from_bytes(raw).unwrap(), expected);
        }
    }
}
