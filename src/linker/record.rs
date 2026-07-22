//! The record of what dealer put into `node_modules`.
//!
//! Without it there is no way to tell a link dealer created from a directory
//! the developer or another tool owns, and an installer that cannot tell the
//! difference has to either clobber or give up.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::Result;
use crate::util::write_atomically;

/// Where the record lives, relative to `node_modules`.
pub const RECORD_PATH: &str = ".dealer/managed.json";

const FORMAT_VERSION: u64 = 1;

/// One package dealer has materialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPackage {
    pub version: String,
    /// The store key the package was materialized from.
    pub store_key: String,
}

/// Everything dealer currently manages inside one `node_modules`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ManagedTree {
    pub packages: BTreeMap<String, ManagedPackage>,
    /// Executable name to the package that provides it.
    pub binaries: BTreeMap<String, String>,
}

impl ManagedTree {
    /// Reads the record, treating a missing or unreadable one as empty. An
    /// unreadable record only costs a relink, so it is never fatal.
    pub fn load(node_modules: &Path) -> Self {
        let Ok(contents) = fs::read(Self::path(node_modules)) else {
            return Self::default();
        };
        let Ok(document) = serde_json::from_slice::<Value>(&contents) else {
            return Self::default();
        };
        if document.get("version").and_then(Value::as_u64) != Some(FORMAT_VERSION) {
            return Self::default();
        }

        Self {
            packages: read_packages(&document),
            binaries: read_binaries(&document),
        }
    }

    pub fn save(&self, node_modules: &Path) -> Result<()> {
        let packages: serde_json::Map<String, Value> = self
            .packages
            .iter()
            .map(|(name, entry)| {
                (
                    name.clone(),
                    json!({ "version": entry.version, "store": entry.store_key }),
                )
            })
            .collect();

        let document = json!({
            "version": FORMAT_VERSION,
            "packages": packages,
            "binaries": self.binaries,
        });

        write_atomically(
            &Self::path(node_modules),
            format!("{}\n", serde_json::to_string_pretty(&document)?).as_bytes(),
        )
    }

    pub fn path(node_modules: &Path) -> PathBuf {
        node_modules.join(RECORD_PATH)
    }
}

fn read_packages(document: &Value) -> BTreeMap<String, ManagedPackage> {
    document
        .get("packages")
        .and_then(Value::as_object)
        .map(|packages| {
            packages
                .iter()
                .filter_map(|(name, entry)| {
                    Some((
                        name.clone(),
                        ManagedPackage {
                            version: entry.get("version")?.as_str()?.to_string(),
                            store_key: entry.get("store")?.as_str()?.to_string(),
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_binaries(document: &Value) -> BTreeMap<String, String> {
    document
        .get("binaries")
        .and_then(Value::as_object)
        .map(|binaries| {
            binaries
                .iter()
                .filter_map(|(name, owner)| Some((name.clone(), owner.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}
