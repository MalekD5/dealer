use serde_json::{Result, Value};

use std::fs;

pub(crate) struct ManifestLoader {
    base_url: String,
    pub json: Result<Value>
}

impl ManifestLoader {
    pub fn new(base_url: String) -> ManifestLoader {
        let path = base_url.clone() + "/package.json";

        let contents = fs::read_to_string(&path).expect("File not found");

        let json = serde_json::from_str::<Value>(&contents);

        ManifestLoader {
            base_url,
            json
        }
    }
}