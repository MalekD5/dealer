//! A content addressed store of extracted packages.
//!
//! Every entry is named after the SHA-512 of the tarball it came from, so two
//! projects that install the same artifact share one extracted copy.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, error};
use crate::tarball::extract::extract_package;
use crate::util::{create_directory, remove_any, temporary_name, write_atomically};

/// Marks an entry as fully extracted. It sits beside the package directory so
/// the package itself stays byte for byte what the tarball contained.
const READY_SUFFIX: &str = ".ready";

const STAGING_DIRECTORY: &str = ".staging";

/// A package directory inside the store.
#[derive(Debug)]
pub struct StoreEntry {
    pub path: PathBuf,
    /// Whether the package was already extracted by an earlier install.
    pub reused: bool,
}

pub struct PackageStore {
    root: PathBuf,
}

impl PackageStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the package with this store key lives, extracted or not.
    pub fn package_path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    /// Whether a complete copy of this package is already stored.
    pub fn contains(&self, key: &str) -> bool {
        self.ready_marker(key).exists() && self.package_path(key).is_dir()
    }

    /// Extracts `tarball` under `key`, reusing an existing entry when there is
    /// one. Repeated calls with the same key do no work.
    pub fn insert(&self, key: &str, tarball: &[u8]) -> Result<StoreEntry> {
        let path = self.package_path(key);
        if self.contains(key) {
            return Ok(StoreEntry { path, reused: true });
        }

        let staging = self.root.join(STAGING_DIRECTORY);
        create_directory(&staging)?;
        let pending = staging.join(temporary_name(Path::new(key)));

        let extracted = extract_package(tarball, &pending);
        if extracted.is_err() {
            let _ = remove_any(&pending);
        }
        extracted?;

        // An entry without its marker is a leftover from an interrupted run.
        remove_any(&path)?;
        if let Err(failure) = fs::rename(&pending, &path) {
            let _ = remove_any(&pending);

            if !path.is_dir() {
                return Err(error(format!(
                    "could not move the extracted package into {}: {failure}",
                    path.display()
                )));
            }
        }

        write_atomically(&self.ready_marker(key), key.as_bytes())?;

        Ok(StoreEntry {
            path,
            reused: false,
        })
    }

    fn ready_marker(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}{READY_SUFFIX}"))
    }
}
