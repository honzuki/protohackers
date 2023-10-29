#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    // Key, Value
    Insert(String, String),
    Retrieve(String),
}

impl Request {
    pub fn from_string(mut raw: String) -> Self {
        match raw.find('=') {
            Some(split_index) => {
                // An insert request formated key=value
                let value = raw.split_off(split_index + 1);
                raw.pop(); // remove the '=' sign from the end
                Self::Insert(raw, value)
            }
            None => {
                // A retreieve request
                Self::Retrieve(raw)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Request;

    #[test]
    fn parse_insert_request() {
        let expetced_values = [
            ("foo", "bar"),
            ("foo", "bar=baz"),
            ("foo", ""),
            ("foo", "=="),
            ("", "foo"),
        ]
        .into_iter()
        .map(|(key, value)| Request::Insert(key.to_string(), value.to_string()));

        let received_values = ["foo=bar", "foo=bar=baz", "foo=", "foo===", "=foo"]
            .into_iter()
            .map(|value| Request::from_string(value.to_string()));

        for (received, expected) in received_values.zip(expetced_values) {
            assert_eq!(received, expected);
        }
    }

    #[test]
    fn parse_retrieve_request() {
        let expetced_values = ["foo", ""]
            .into_iter()
            .map(|key| Request::Retrieve(key.to_string()));

        let received_values = ["foo", ""]
            .into_iter()
            .map(|key| Request::from_string(key.to_string()));

        for (received, expected) in received_values.zip(expetced_values) {
            assert_eq!(received, expected);
        }
    }
}
