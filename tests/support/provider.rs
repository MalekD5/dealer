//! An in-memory registry for exercising resolution rules without a network.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use dealer::error::{Result, error};
use dealer::hash::Sha512;
use dealer::package::{HashAlgorithm, Integrity, ResolvedPackage, TarballLocation};
use dealer::resolver::{PackageProvider, VersionIndex};
use dealer::tarball::read_manifest;

#[derive(Default)]
pub struct FakeProvider {
    packages: BTreeMap<String, BTreeMap<String, ResolvedPackage>>,
    tags: BTreeMap<String, BTreeMap<String, String>>,
    lookups: RefCell<Vec<String>>,
}

impl FakeProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publishes a version with the given dependency ranges.
    pub fn publish(mut self, name: &str, version: &str, dependencies: &[(&str, &str)]) -> Self {
        let package = ResolvedPackage {
            name: name.to_string(),
            version: version.to_string(),
            integrity: None,
            tarball: TarballLocation::Remote(format!(
                "https://registry.test/{name}/-/{name}-{version}.tgz"
            )),
            dependencies: dependencies
                .iter()
                .map(|(name, range)| ((*name).to_string(), (*range).to_string()))
                .collect(),
            bin: BTreeMap::new(),
        };

        self.packages
            .entry(name.to_string())
            .or_default()
            .insert(version.to_string(), package);
        self
    }

    pub fn tag(mut self, name: &str, tag: &str, version: &str) -> Self {
        self.tags
            .entry(name.to_string())
            .or_default()
            .insert(tag.to_string(), version.to_string());
        self
    }

    /// The package names whose version list has been asked for, in order.
    pub fn lookups(&self) -> Vec<String> {
        self.lookups.borrow().clone()
    }
}

impl PackageProvider for FakeProvider {
    fn versions(&self, name: &str) -> Result<VersionIndex> {
        self.lookups.borrow_mut().push(name.to_string());

        let versions = self.packages.get(name).ok_or_else(|| {
            error(format!(
                "package `{name}` was not found in the test registry"
            ))
        })?;

        Ok(VersionIndex {
            versions: versions.keys().cloned().collect(),
            tags: self.tags.get(name).cloned().unwrap_or_default(),
        })
    }

    fn package(&self, name: &str, version: &str) -> Result<ResolvedPackage> {
        self.packages
            .get(name)
            .and_then(|versions| versions.get(version))
            .cloned()
            .ok_or_else(|| error(format!("`{name}@{version}` is not published")))
    }

    fn local_package(&self, path: &Path) -> Result<ResolvedPackage> {
        let bytes = std::fs::read(path)
            .map_err(|failure| error(format!("could not read {}: {failure}", path.display())))?;
        let metadata = read_manifest(&bytes)?;
        let integrity = Integrity::new(HashAlgorithm::Sha512, Sha512::digest(&bytes).to_vec())
            .map_err(error)?;

        ResolvedPackage::from_metadata(
            &metadata,
            TarballLocation::Local(path.to_path_buf()),
            Some(integrity),
        )
        .map_err(error)
    }
}
