use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{Result, error};

pub const MANIFEST_FILE: &str = "package.json";

/// The `package.json` belonging to a project directory.
pub fn manifest_path(directory: &Path) -> PathBuf {
    directory.join(MANIFEST_FILE)
}

/// Reads and parses a project's `package.json`.
pub fn load_document(directory: &Path) -> Result<Value> {
    let path = manifest_path(directory);

    let contents = fs::read_to_string(&path).map_err(|failure| match failure.kind() {
        ErrorKind::NotFound => error(format!(
            "no {MANIFEST_FILE} in {}; run `dealer init` to create one",
            directory.display()
        )),
        _ => error(format!("could not read {}: {failure}", path.display())),
    })?;

    serde_json::from_str(&contents)
        .map_err(|failure| error(format!("failed to parse JSON: {failure}")))
}
