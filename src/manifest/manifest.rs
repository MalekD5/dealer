use std::collections::HashMap;

use serde_json::Value;

use crate::manifest::json_loader::ManifestLoader;

pub struct PackageManifest {
    pub name: String,
    pub version: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub scripts: HashMap<String, String>,
}

impl PackageManifest {
    pub fn new(base_url: String) -> Result<PackageManifest, String> {
        let manifest_loader = ManifestLoader::new(base_url);

        let data = manifest_loader
            .json
            .map_err(|error| format!("failed to parse JSON: {error}"))?;

        let name = data
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "`name` is missing or is not a string".to_string())?
            .to_string();

        let version = data
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| "`version` is missing or is not a string".to_string())?
            .to_string();

        let description = data
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);

        let scripts = match data.get("scripts") {
            Some(value) => serde_json::from_value::<HashMap<String, String>>(value.clone())
                .map_err(|error| format!("`scripts` must contain only string values: {error}"))?,
            None => HashMap::new(),
        };

        Ok(PackageManifest {
            name,
            version,
            description,
            scripts,
        })
    }
}
