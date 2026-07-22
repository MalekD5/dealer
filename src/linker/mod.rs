//! Dependency linking support.
//!
//! Materializes store packages into a project's `node_modules` and keeps a
//! record of what it put there, so a later install can replace what dealer
//! owns without disturbing anything it does not.

pub mod bin;
pub mod record;

pub use record::{ManagedPackage, ManagedTree};

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, error};
use crate::package::ResolvedPackage;
use crate::util::{clone_tree, create_directory, remove_any, safe_components};

/// One package to materialize, paired with where it sits in the store.
#[derive(Debug)]
pub struct LinkRequest<'a> {
    pub package: &'a ResolvedPackage,
    pub store_key: String,
    pub store_path: PathBuf,
}

/// What one linking pass changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinkReport {
    /// Packages materialized during this pass, as `name@version`.
    pub linked: Vec<String>,
    /// Packages that were already present at the right version.
    pub unchanged: Vec<String>,
    /// Packages dealer removed because the plan no longer includes them.
    pub removed: Vec<String>,
    /// Directories dealer refused to touch because it did not create them.
    pub skipped: Vec<String>,
    /// Executables now available in `node_modules/.bin`.
    pub binaries: Vec<String>,
}

pub struct Linker {
    node_modules: PathBuf,
}

impl Linker {
    pub fn new(node_modules: PathBuf) -> Self {
        Self { node_modules }
    }

    pub fn node_modules(&self) -> &Path {
        &self.node_modules
    }

    /// Brings `node_modules` in line with the requested set of packages.
    pub fn link(&self, requests: &[LinkRequest<'_>]) -> Result<LinkReport> {
        create_directory(&self.node_modules)?;

        let mut managed = ManagedTree::load(&self.node_modules);
        let mut report = LinkReport::default();

        self.remove_dropped_packages(requests, &mut managed, &mut report)?;
        let binaries = self.materialize_packages(requests, &mut managed, &mut report)?;
        self.refresh_shims(&binaries, &mut managed, &mut report)?;

        managed.save(&self.node_modules)?;

        Ok(report)
    }

    /// Drops packages dealer installed earlier that the plan no longer wants.
    fn remove_dropped_packages(
        &self,
        requests: &[LinkRequest<'_>],
        managed: &mut ManagedTree,
        report: &mut LinkReport,
    ) -> Result<()> {
        let wanted: BTreeSet<&str> = requests
            .iter()
            .map(|request| request.package.name.as_str())
            .collect();
        let dropped: Vec<String> = managed
            .packages
            .keys()
            .filter(|name| !wanted.contains(name.as_str()))
            .cloned()
            .collect();

        for name in dropped {
            self.remove_package(&name)?;
            managed.packages.remove(&name);
            report.removed.push(name);
        }

        Ok(())
    }

    /// Materializes every requested package, collecting the executables they
    /// declare along the way.
    fn materialize_packages(
        &self,
        requests: &[LinkRequest<'_>],
        managed: &mut ManagedTree,
        report: &mut LinkReport,
    ) -> Result<BTreeMap<String, String>> {
        let mut binaries = BTreeMap::new();

        for request in requests {
            let package = request.package;
            let target = self.package_path(&package.name);
            let previously = managed.packages.get(&package.name);

            let current = previously.is_some_and(|entry| entry.store_key == request.store_key)
                && target.exists();

            if !current {
                // Anything dealer has no record of belongs to somebody else.
                if previously.is_none() && target.exists() {
                    report.skipped.push(package.name.clone());
                    continue;
                }

                if let Some(parent) = target.parent() {
                    create_directory(parent)?;
                }
                remove_any(&target)?;

                // Hard linking is used in preference to a directory symlink
                // because node resolves `require` through symlinks: a
                // symlinked package would look for its own dependencies beside
                // the store rather than in the project.
                clone_tree(&request.store_path, &target)?;
                report.linked.push(package.id());
            } else {
                report.unchanged.push(package.id());
            }

            managed.packages.insert(
                package.name.clone(),
                ManagedPackage {
                    version: package.version.clone(),
                    store_key: request.store_key.clone(),
                },
            );

            // Requests arrive in plan order, so the first package to claim an
            // executable name keeps it however the install was invoked.
            for (name, path) in &package.bin {
                if binaries.contains_key(name) {
                    continue;
                }

                binaries.insert(name.clone(), shim_target(&package.name, path)?);
            }
        }

        Ok(binaries)
    }

    fn refresh_shims(
        &self,
        binaries: &BTreeMap<String, String>,
        managed: &mut ManagedTree,
        report: &mut LinkReport,
    ) -> Result<()> {
        let stale: Vec<String> = managed
            .binaries
            .keys()
            .filter(|name| !binaries.contains_key(*name))
            .cloned()
            .collect();

        for name in stale {
            bin::remove_shim(&self.node_modules, &name)?;
            managed.binaries.remove(&name);
        }

        for (name, target) in binaries {
            bin::write_shim(&self.node_modules, name, target)?;
            managed.binaries.insert(name.clone(), target.clone());
            report.binaries.push(name.clone());
        }

        Ok(())
    }

    /// Removes a package, and the scope directory it lived in once empty.
    fn remove_package(&self, name: &str) -> Result<()> {
        let path = self.package_path(name);
        remove_any(&path)?;

        if name.starts_with('@')
            && let Some(scope) = path.parent()
        {
            let empty = fs::read_dir(scope)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);

            if empty {
                remove_any(scope)?;
            }
        }

        Ok(())
    }

    /// Where a package lives, with `@scope/name` split into two directories.
    fn package_path(&self, name: &str) -> PathBuf {
        name.split('/')
            .fold(self.node_modules.clone(), |path, part| path.join(part))
    }
}

/// The path a shim points at, relative to `node_modules`.
///
/// A package could declare a `bin` path that climbs out of its own directory,
/// so the declaration is validated rather than trusted.
fn shim_target(package: &str, declared: &str) -> Result<String> {
    let components = safe_components(declared)
        .map_err(|failure| error(format!("`{package}` declares a bin path that {failure}")))?;

    Ok(format!("{package}/{}", components.join("/")))
}
