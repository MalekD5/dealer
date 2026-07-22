use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{Result, error};
use crate::hash::Sha512;
use crate::package::{HashAlgorithm, Integrity, ResolvedPackage, TarballLocation};
use crate::registry::RegistryClient;
use crate::tarball::{TarballCache, read_manifest};

/// What a registry publishes about which versions of a package exist.
#[derive(Debug, Default, Clone)]
pub struct VersionIndex {
    pub versions: Vec<String>,
    /// Named aliases such as `latest`.
    pub tags: BTreeMap<String, String>,
}

/// Supplies the metadata the resolver reasons about.
///
/// Keeping this behind a trait lets the resolution rules be exercised without
/// a registry, and keeps network access out of the graph walk.
pub trait PackageProvider {
    /// Every version published for `name`, along with its dist-tags.
    fn versions(&self, name: &str) -> Result<VersionIndex>;

    /// Metadata for one concrete version.
    fn package(&self, name: &str, version: &str) -> Result<ResolvedPackage>;

    /// Reads a `.tgz` from disk, which is the only way to learn what it holds.
    fn local_package(&self, path: &Path) -> Result<ResolvedPackage>;
}

/// The provider used for real installs.
pub struct RegistryProvider<'a> {
    client: &'a RegistryClient,
    cache: &'a TarballCache<'a>,
}

impl<'a> RegistryProvider<'a> {
    pub fn new(client: &'a RegistryClient, cache: &'a TarballCache<'a>) -> Self {
        Self { client, cache }
    }
}

impl PackageProvider for RegistryProvider<'_> {
    fn versions(&self, name: &str) -> Result<VersionIndex> {
        let packument = self.client.packument(name)?;

        Ok(VersionIndex {
            versions: packument.versions().map(str::to_string).collect(),
            tags: packument.dist_tags.clone(),
        })
    }

    fn package(&self, name: &str, version: &str) -> Result<ResolvedPackage> {
        self.client.packument(name)?.resolve(version)
    }

    fn local_package(&self, path: &Path) -> Result<ResolvedPackage> {
        let path = absolute(path)?;
        let artifact = self.cache.read_local(&path, None)?;
        let metadata = read_manifest(&artifact.bytes)
            .map_err(|failure| error(format!("{}: {failure}", path.display())))?;

        // A local tarball carries no published checksum, so it is its own.
        let integrity = Integrity::new(
            HashAlgorithm::Sha512,
            Sha512::digest(&artifact.bytes).to_vec(),
        )
        .map_err(error)?;

        ResolvedPackage::from_metadata(&metadata, TarballLocation::Local(path), Some(integrity))
            .map_err(error)
    }
}

/// Makes a path absolute without resolving symlinks, so plans do not depend on
/// the working directory.
fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path)
        .map_err(|failure| error(format!("could not resolve {}: {failure}", path.display())))
}
