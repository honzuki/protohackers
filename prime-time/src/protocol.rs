use serde::{Deserialize, Serialize};
use thiserror::Error;

const METHOD_NAME: &str = "isPrime";
pub const MALFORMED_RESPONSE: &str = "{}";

#[derive(Error, Debug)]
pub enum ParseRequestError {
    #[error("{0}")]
    Parse(#[from] serde_json::Error),
    #[error("Unknown method name: {0}")]
    UnknownMethod(String),
}

#[derive(Deserialize)]
pub struct Request {
    method: String,
    number: serde_json::value::Number,
}

#[derive(Serialize)]
pub struct Response {
    method: String,
    prime: bool,
}

impl Response {
    pub fn new(is_prime: bool) -> Self {
        Self {
            method: METHOD_NAME.into(),
            prime: is_prime,
        }
    }
}

pub fn get_number_from_request(request: &str) -> Result<Option<u64>, ParseRequestError> {
    let req: Request = serde_json::from_str(request)?;
    if req.method != METHOD_NAME {
        return Err(ParseRequestError::UnknownMethod(req.method));
    }

    Ok(req.number.as_u64())
}
