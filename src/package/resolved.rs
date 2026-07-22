use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde_json::Value;

use super::integrity::Integrity;

/// Where the tarball for a resolved package can be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TarballLocation {
    /// An `https://` (or `http://`) URL served by a registry.
    Remote(String),
    /// A `.tgz` archive that already exists on disk.
    Local(PathBuf),
}

impl fmt::Display for TarballLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TarballLocation::Remote(url) => formatter.write_str(url),
            TarballLocation::Local(path) => write!(formatter, "{}", path.display()),
        }
    }
}

/// A dependency narrowed down to a single publishable artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackage {
    pub name: String,
    pub version: String,
    /// Absent when the registry publishes no checksum, or for local tarballs
    /// that have not been hashed yet.
    pub integrity: Option<Integrity>,
    pub tarball: TarballLocation,
    /// Runtime dependencies that must be resolved transitively.
    pub dependencies: BTreeMap<String, String>,
    /// Executable name to script path, relative to the package root.
    pub bin: BTreeMap<String, String>,
}

impl ResolvedPackage {
    /// Reads a `package.json`-shaped object, as published inside a packument
    /// version entry or inside a tarball.
    pub fn from_metadata(
        metadata: &Value,
        tarball: TarballLocation,
        integrity: Option<Integrity>,
    ) -> Result<Self, String> {
        let name = metadata
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "package metadata is missing a string `name`".to_string())?
            .to_string();
        let version = metadata
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("package `{name}` is missing a string `version`"))?
            .to_string();

        Ok(Self {
            dependencies: read_dependencies(metadata, "dependencies"),
            bin: read_bin(metadata, &name),
            name,
            version,
            integrity,
            tarball,
        })
    }

    /// The `name@version` identity used in messages and plan ordering.
    pub fn id(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    /// A deterministic, content addressed directory name for the store.
    ///
    /// `tarball_digest` is the hexadecimal SHA-512 of the tarball bytes, so two
    /// packages that publish identical archives share a store entry.
    pub fn store_key(&self, tarball_digest: &str) -> String {
        let digest: String = tarball_digest.chars().take(16).collect();
        format!(
            "{}@{}-{}",
            path_safe(&self.name),
            path_safe(&self.version),
            digest
        )
    }
}

impl fmt::Display for ResolvedPackage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.id())
    }
}

/// Reads a `{ "name": "range" }` dependency table, ignoring non-string entries.
pub fn read_dependencies(metadata: &Value, key: &str) -> BTreeMap<String, String> {
    metadata
        .get(key)
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(name, range)| Some((name.clone(), range.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads the `bin` field, which npm allows to be either a single path or a
/// table of executable names.
pub fn read_bin(metadata: &Value, package_name: &str) -> BTreeMap<String, String> {
    match metadata.get("bin") {
        Some(Value::String(path)) => {
            let name = package_name.rsplit('/').next().unwrap_or(package_name);
            BTreeMap::from([(name.to_string(), path.clone())])
        }
        Some(Value::Object(entries)) => entries
            .iter()
            .filter_map(|(name, path)| Some((name.clone(), path.as_str()?.to_string())))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Replaces characters that are not portable in a directory name.
fn path_safe(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '+',
            other => other,
        })
        .collect()
}
