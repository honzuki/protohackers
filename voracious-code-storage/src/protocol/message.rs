use async_tempfile::TempFile;

use crate::storage::ListResult;

#[derive(Debug)]
pub enum Request {
    Put {
        filename: String,
        file: TempFile,
        hash: Vec<u8>,
    },
    Get {
        filename: String,
        revision: Option<u64>,
    },
    List {
        path: String,
    },
    Help,
}

#[derive(Debug)]
pub struct Response {
    pub(super) raw: raw::Response,
}

impl Response {
    pub fn error(reason: String) -> Self {
        Self {
            raw: raw::Response::Err(reason),
        }
    }

    pub fn get(file: TempFile) -> Self {
        Self {
            raw: raw::Response::Get { file },
        }
    }

    pub fn put(revision: u64) -> Self {
        Self {
            raw: raw::Response::Put { revision },
        }
    }

    pub fn list(children: Vec<ListResult>) -> Self {
        Self {
            raw: raw::Response::List { children },
        }
    }

    pub fn help() -> Self {
        Self {
            raw: raw::Response::Help,
        }
    }
}
// Raw structures for internal use
pub(super) mod raw {
    use std::str::FromStr;

    use async_tempfile::TempFile;

    use crate::storage::ListResult;

    const PUT_USAGE_MSG: &str = "PUT file length newline data";
    const GET_USAGE_MSG: &str = "GET file [revision]";
    const LIST_USAGE_MSG: &str = "LIST dir";

    #[derive(Debug)]
    pub enum Response {
        Put { revision: u64 },
        Get { file: TempFile },
        List { children: Vec<ListResult> },
        Help,
        Err(String),
    }

    #[derive(Debug, PartialEq)]
    pub enum Request {
        Put {
            filename: String,
            byte_count: u64,
        },
        Get {
            filename: String,
            revision: Option<u64>,
        },
        List {
            path: String,
        },
        Help,
    }

    #[derive(thiserror::Error, Debug)]
    pub enum RequestErr {
        #[error("illegal method: {0}")]
        IllegalMethod(String),

        #[error("usage: {0}")]
        BadUsage(String),

        #[error("illegal file name")]
        IllegalFileName,

        #[error("illegal dir name")]
        IllegalDirName,
    }

    impl FromStr for Request {
        type Err = RequestErr;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let mut parts = s.trim().split_ascii_whitespace();

            let method = parts
                .next()
                // method is case insensitive
                .map(|method| method.to_uppercase())
                .unwrap_or_default();

            match method.as_str() {
                "PUT" => {
                    // parse put request
                    let filename: String = parts
                        .next()
                        .ok_or_else(|| RequestErr::BadUsage(PUT_USAGE_MSG.into()))?
                        .into();
                    if !check_filename(&filename) {
                        return Err(RequestErr::IllegalFileName);
                    }

                    let byte_count = parts
                        .next()
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| RequestErr::BadUsage(PUT_USAGE_MSG.into()))?;

                    // make sure we've consumed the entire line
                    if parts.next().is_some() {
                        return Err(RequestErr::BadUsage(PUT_USAGE_MSG.into()));
                    }

                    Ok(Self::Put {
                        filename,
                        byte_count,
                    })
                }
                "GET" => {
                    // parse get request
                    let filename: String = parts
                        .next()
                        .ok_or_else(|| RequestErr::BadUsage(GET_USAGE_MSG.into()))?
                        .into();
                    if !check_filename(&filename) {
                        return Err(RequestErr::IllegalFileName);
                    }

                    let revision = parts
                        .next()
                        .map(|value| value.strip_prefix('r').unwrap_or(value));

                    let revision = match revision {
                        Some(revision) => Some(
                            revision
                                .parse()
                                .map_err(|_| RequestErr::BadUsage(GET_USAGE_MSG.into()))?,
                        ),
                        None => None,
                    };

                    // make sure we've consumed the entire line
                    if parts.next().is_some() {
                        return Err(RequestErr::BadUsage(GET_USAGE_MSG.into()));
                    }

                    Ok(Self::Get { filename, revision })
                }
                "LIST" => {
                    let path: String = validate_dirpath(
                        parts
                            .next()
                            .ok_or_else(|| RequestErr::BadUsage(LIST_USAGE_MSG.into()))?
                            .into(),
                    )?;

                    // make sure we've consumed the entire line
                    if parts.next().is_some() {
                        return Err(RequestErr::BadUsage(LIST_USAGE_MSG.into()));
                    }

                    Ok(Self::List { path })
                }
                "HELP" => Ok(Self::Help),
                _ => Err(RequestErr::IllegalMethod(method.to_string())),
            }
        }
    }

    // checks that the filename matches the expected format
    fn check_filename(filename: &str) -> bool {
        // files should always start at root
        if !filename.starts_with('/') {
            return false;
        }

        let filename = &filename[1..];

        // file name can not be empty
        if filename.trim().is_empty() {
            return false;
        }

        // each part of the path most contain something
        if !validate_strippted_path(filename) {
            return false;
        }

        true
    }

    // checks that a dir name matches the expected format
    // and return a unified view of this dir
    fn validate_dirpath(mut dir: String) -> Result<String, RequestErr> {
        // dir path should always start at root
        if !dir.starts_with('/') {
            return Err(RequestErr::IllegalDirName);
        }

        // check for proper naming
        if !dir
            .chars()
            .all(|char| char.is_alphanumeric() || char == '.' || char == '_' || char == '/')
        {
            return Err(RequestErr::IllegalDirName);
        }

        // dir may, or may not, end with a '/'
        if !dir.ends_with('/') {
            dir.push('/');
        }

        // each part of the path most contain something and be one of "alphanumeric, dot, underscore"
        if dir.len() > 1 && !validate_strippted_path(&dir[1..dir.len() - 1]) {
            return Err(RequestErr::IllegalDirName);
        }

        Ok(dir)
    }

    fn validate_strippted_path(path: &str) -> bool {
        path.split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|char| char.is_alphanumeric() || char == '.' || char == '_' || char == '-')
        })
    }

    #[cfg(test)]
    mod tests {
        use super::Request;

        #[test]
        fn check_valid_request_parsing() {
            let raw_requests = [
                "puT /test.txt 35",
                "GEt /text.txt",
                "GeT /text.txt 90",
                "gET /text.txt r5",
                "LIST /test/",
                "LIST /test/test2/test44/../test5",
                "PuT /v.-WC1CDakNoPWm4YiOxD7p-F2VC8-AahIWXRQ/gHDhPY8euDkFdTa3lo5oPsV7-KpOQKknmnNSRHX4jKxm9omKLVrZPB3WIQ27nLB.h2KjsMx-q5H_GU0F9eIXyFPcgu 57"
            ];

            let expected_requests = [
                Request::Put {
                    filename: "/test.txt".into(),
                    byte_count: 35,
                },
                Request::Get {
                    filename: "/text.txt".into(),
                    revision: None,
                },
                Request::Get {
                    filename: "/text.txt".into(),
                    revision: Some(90),
                },
                Request::Get {
                    filename: "/text.txt".into(),
                    revision: Some(5),
                },
                Request::List {
                    path: "/test/".into(),
                },
                Request::List {
                    path: "/test/test2/test44/../test5/".into(),
                },
                Request::Put { filename: "/v.-WC1CDakNoPWm4YiOxD7p-F2VC8-AahIWXRQ/gHDhPY8euDkFdTa3lo5oPsV7-KpOQKknmnNSRHX4jKxm9omKLVrZPB3WIQ27nLB.h2KjsMx-q5H_GU0F9eIXyFPcgu".into(), byte_count: 57 }
            ];

            for (request, expected) in raw_requests.into_iter().zip(expected_requests.iter()) {
                let request = match request.parse::<Request>() {
                    Ok(request) => request,
                    Err(reason) => panic!("failed to parse\n{}\nreason: {}", request, reason),
                };

                assert_eq!(request, *expected);
            }
        }

        #[test]
        fn check_bad_request_parsing() {
            let bad_request = [
                "PUT /text.txt",
                "PUT /text abc",
                "PUT /text r2",
                "GET /text\\. text",
                "GET /text.txt 123 123",
                "GET /text/ 12",
                "GET /text//test 12",
                "LIST /test//",
                "LIST",
                "LISt /test//test/",
                "LiSt /test/../test//",
                "PuT PUT /mbA+u|=]hj)oMraH0pS 123",
            ];

            for request in bad_request {
                let request: Result<Request, _> = request.parse();
                assert!(request.is_err())
            }
        }
    }
}
