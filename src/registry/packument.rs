use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::{Result, error};
use crate::package::{Integrity, ResolvedPackage, TarballLocation};
use crate::semver::select_version;

/// How many candidate versions are listed when nothing satisfies a range.
const REPORTED_VERSIONS: usize = 8;

/// The version metadata a registry publishes for a single package.
#[derive(Debug, Clone)]
pub struct Packument {
    pub name: String,
    /// Named aliases such as `latest` or `next`.
    pub dist_tags: BTreeMap<String, String>,
    /// Version number to its `package.json`-shaped metadata.
    versions: BTreeMap<String, Value>,
}

impl Packument {
    pub fn parse(name: &str, document: &Value) -> Result<Self> {
        let versions = document
            .get("versions")
            .and_then(Value::as_object)
            .ok_or_else(|| error(format!("registry metadata for `{name}` has no `versions`")))?
            .iter()
            .map(|(version, metadata)| (version.clone(), metadata.clone()))
            .collect();

        let dist_tags = document
            .get("dist-tags")
            .and_then(Value::as_object)
            .map(|tags| {
                tags.iter()
                    .filter_map(|(tag, version)| Some((tag.clone(), version.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            name: name.to_string(),
            dist_tags,
            versions,
        })
    }

    /// Every published version, in ascending string order.
    pub fn versions(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.versions.keys().map(String::as_str)
    }

    /// Narrows a dist-tag or semver range down to a single publishable
    /// artifact.
    pub fn resolve(&self, range: &str) -> Result<ResolvedPackage> {
        let version = self.select(range)?;
        let metadata = self
            .versions
            .get(&version)
            .ok_or_else(|| error(format!("`{}@{version}` is not published", self.name)))?;

        let distribution = metadata.get("dist").ok_or_else(|| {
            error(format!(
                "`{}@{version}` has no `dist` block to download from",
                self.name
            ))
        })?;
        let tarball = distribution
            .get("tarball")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                error(format!(
                    "`{}@{version}` does not publish a tarball URL",
                    self.name
                ))
            })?;

        let integrity = read_integrity(distribution)
            .map_err(|reason| error(format!("`{}@{version}`: {reason}", self.name)))?;

        ResolvedPackage::from_metadata(
            metadata,
            TarballLocation::Remote(tarball.to_string()),
            integrity,
        )
        .map_err(error)
    }

    fn select(&self, range: &str) -> Result<String> {
        if let Some(version) = self.dist_tags.get(range) {
            return Ok(version.clone());
        }

        let candidates: Vec<String> = self.versions.keys().cloned().collect();
        let matches = select_version(range.to_string(), candidates).map_err(|error| {
            crate::error::error(format!(
                "`{range}` is not a valid range for `{}`: {error}",
                self.name
            ))
        })?;

        matches.into_iter().next().ok_or_else(|| {
            error(format!(
                "no published version of `{}` satisfies `{range}` (available: {})",
                self.name,
                self.describe_versions()
            ))
        })
    }

    fn describe_versions(&self) -> String {
        let total = self.versions.len();
        let listed: Vec<&str> = self.versions().rev().take(REPORTED_VERSIONS).collect();
        let mut described = listed
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(", ");

        if total > REPORTED_VERSIONS {
            described = format!("{} more, {described}", total - REPORTED_VERSIONS);
        }
        if described.is_empty() {
            described = "none".to_string();
        }
        described
    }
}

/// Prefers modern `dist.integrity` and falls back to the legacy `dist.shasum`.
fn read_integrity(distribution: &Value) -> std::result::Result<Option<Integrity>, String> {
    if let Some(value) = distribution.get("integrity").and_then(Value::as_str) {
        return Integrity::parse(value).map(Some);
    }
    if let Some(value) = distribution.get("shasum").and_then(Value::as_str) {
        return Integrity::from_shasum(value).map(Some);
    }

    Ok(None)
}
