//! Tarball acquisition backed by a deterministic on-disk cache.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, error};
use crate::hash::{Sha512, to_hex};
use crate::package::{Integrity, ResolvedPackage, TarballLocation};
use crate::registry::RegistryClient;
use crate::util::write_atomically;

/// How many hexadecimal digits of a digest go into a cache file name.
const KEY_LENGTH: usize = 16;

/// A tarball that is ready to be verified and extracted.
#[derive(Debug)]
pub struct Artifact {
    pub bytes: Vec<u8>,
    /// Hexadecimal SHA-512 of the tarball, used to key the package store.
    pub digest: String,
    /// Whether the bytes came from the cache rather than the network.
    pub reused: bool,
}

/// Fetches tarballs, keeping remote downloads in a shared cache directory.
pub struct TarballCache<'a> {
    root: PathBuf,
    client: &'a RegistryClient,
}

impl<'a> TarballCache<'a> {
    pub fn new(root: PathBuf, client: &'a RegistryClient) -> Self {
        Self { root, client }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Produces the bytes for a resolved package, downloading them only when
    /// the cache cannot supply a valid copy.
    pub fn acquire(&self, package: &ResolvedPackage) -> Result<Artifact> {
        match &package.tarball {
            TarballLocation::Local(path) => self.read_local(path, package.integrity.as_ref()),
            TarballLocation::Remote(url) => self.fetch_remote(package, url),
        }
    }

    /// Reads a `.tgz` that already exists on disk.
    pub fn read_local(&self, path: &Path, integrity: Option<&Integrity>) -> Result<Artifact> {
        let bytes = fs::read(path).map_err(|failure| {
            error(format!(
                "could not read tarball {}: {failure}",
                path.display()
            ))
        })?;

        if let Some(integrity) = integrity {
            integrity
                .verify(&bytes)
                .map_err(|reason| error(format!("{}: {reason}", path.display())))?;
        }

        Ok(Artifact {
            digest: to_hex(&Sha512::digest(&bytes)),
            bytes,
            reused: true,
        })
    }

    fn fetch_remote(&self, package: &ResolvedPackage, url: &str) -> Result<Artifact> {
        let path = self.cache_path(package, url);

        if let Some(bytes) = self.read_cached(&path, package.integrity.as_ref()) {
            return Ok(Artifact {
                digest: to_hex(&Sha512::digest(&bytes)),
                bytes,
                reused: true,
            });
        }

        let bytes = self.client.download(url)?;
        if let Some(integrity) = &package.integrity {
            integrity
                .verify(&bytes)
                .map_err(|reason| error(format!("{}: {reason}", package.id())))?;
        }

        write_atomically(&path, &bytes)?;

        Ok(Artifact {
            digest: to_hex(&Sha512::digest(&bytes)),
            bytes,
            reused: false,
        })
    }

    /// A cache hit only counts when the bytes still satisfy the checksum the
    /// registry published; anything else is treated as a miss.
    fn read_cached(&self, path: &Path, integrity: Option<&Integrity>) -> Option<Vec<u8>> {
        let bytes = fs::read(path).ok()?;

        match integrity {
            Some(integrity) if integrity.verify(&bytes).is_err() => None,
            _ => Some(bytes),
        }
    }

    /// Cache entries are named after the package and a digest of its identity,
    /// so the same request always lands on the same file.
    fn cache_path(&self, package: &ResolvedPackage, url: &str) -> PathBuf {
        let identity = match &package.integrity {
            Some(integrity) => integrity.to_string(),
            None => url.to_string(),
        };
        let key: String = to_hex(&Sha512::digest(identity.as_bytes()))
            .chars()
            .take(KEY_LENGTH)
            .collect();

        self.root.join(format!("{}-{key}.tgz", file_stem(package)))
    }
}

/// `@scope/name` cannot be a single path segment, so scopes are flattened the
/// way npm's own cache does it.
fn file_stem(package: &ResolvedPackage) -> String {
    format!(
        "{}-{}",
        package.name.replace(['/', '\\'], "+"),
        package.version.replace(['/', '\\'], "+")
    )
}
