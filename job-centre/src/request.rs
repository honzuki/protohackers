use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "request", rename_all = "kebab-case")]
pub enum Request {
    Put {
        queue: String,
        job: serde_json::Value,
        #[serde(rename = "pri")]
        priority: u64,
    },
    Get {
        queues: Vec<String>,
        #[serde(default)]
        wait: bool,
    },
    Delete {
        id: u64,
    },
    Abort {
        id: u64,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Response {
    Ok {
        id: Option<u64>,
        queue: Option<String>,
        job: Option<serde_json::Value>,
        #[serde(rename = "pri")]
        priority: Option<u64>,
    },
    Error {
        error: Option<String>,
    },
    NoJob,
}

impl Response {
    pub fn error(reason: String) -> Self {
        Self::Error {
            error: Some(reason),
        }
    }

    pub fn created(id: u64) -> Self {
        Self::Ok {
            id: Some(id),
            queue: None,
            job: None,
            priority: None,
        }
    }

    pub fn job(id: u64, queue: String, job: serde_json::Value, priority: u64) -> Self {
        Self::Ok {
            id: Some(id),
            queue: Some(queue),
            job: Some(job),
            priority: Some(priority),
        }
    }

    pub fn ok() -> Self {
        Self::Ok {
            id: None,
            queue: None,
            job: None,
            priority: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::request::Response;

    use super::Request;

    #[test]
    fn check_structure_definition() {
        let requests = [
            r#"{"request":"put","queue":"queue1","job":{"title":"example-job"},"pri":123}"#,
            r#"{"request":"get","queues":["queue1"]}"#,
            r#"{"request":"abort","id":12345}"#,
            r#"{"request":"delete","id":12345}"#,
            r#"{"request":"get","queues":["queue1"],"wait":true}"#,
        ];

        let expected_requests = [
            Request::Put {
                queue: "queue1".into(),
                job: json!({"title": "example-job"}),
                priority: 123,
            },
            Request::Get {
                queues: ["queue1".into()].into(),
                wait: false,
            },
            Request::Abort { id: 12345 },
            Request::Delete { id: 12345 },
            Request::Get {
                queues: ["queue1".into()].into(),
                wait: true,
            },
        ];

        for (request, expected) in requests.into_iter().zip(expected_requests.into_iter()) {
            let request: Request = serde_json::from_str(request).unwrap();
            assert_eq!(request, expected);
        }

        let responses = [
            r#"{"status":"ok","id":12345}"#,
            r#"{"status":"ok","id":12345,"job":{"title":"example-job"},"pri":123,"queue":"queue1"}"#,
            r#"{"status":"ok"}"#,
            r#"{"status":"no-job"}"#,
        ];

        let expected_responses = [
            Response::Ok {
                id: Some(12345),
                queue: None,
                job: None,
                priority: None,
            },
            Response::Ok {
                id: Some(12345),
                queue: Some("queue1".into()),
                job: Some(json!({"title": "example-job"})),
                priority: Some(123),
            },
            Response::Ok {
                id: None,
                queue: None,
                job: None,
                priority: None,
            },
            Response::NoJob,
        ];

        for (response, expected) in responses.into_iter().zip(expected_responses.into_iter()) {
            let response: Response = serde_json::from_str(response).unwrap();
            assert_eq!(response, expected);
        }
    }
}
