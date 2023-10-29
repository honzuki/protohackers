use dashmap::DashMap;

static RESERVED_KEYS: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "version" => "Ken's Key-Value Store 1.0",
};

#[derive(Debug, Default)]
pub struct KeyValue(DashMap<String, String>);

impl KeyValue {
    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(value) = RESERVED_KEYS.get(key) {
            return Some(value.to_string());
        }

        self.0.get(key).map(|value| value.to_owned())
    }

    pub fn set(&self, key: String, value: String) {
        self.0.insert(key, value);
    }
}
