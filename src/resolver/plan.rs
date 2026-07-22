use std::collections::{BTreeMap, BTreeSet};

use crate::package::ResolvedPackage;

/// The complete, ordered set of packages an install will materialize.
///
/// dealer installs one version of each package, so a plan is keyed by name and
/// its ordering is fixed: the same inputs always produce the same plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallPlan {
    packages: BTreeMap<String, ResolvedPackage>,
    roots: BTreeSet<String>,
}

impl InstallPlan {
    pub(super) fn new(
        packages: BTreeMap<String, ResolvedPackage>,
        roots: BTreeSet<String>,
    ) -> Self {
        Self { packages, roots }
    }

    /// Every package to install, ordered by name.
    pub fn packages(&self) -> impl Iterator<Item = &ResolvedPackage> {
        self.packages.values()
    }

    /// The names the project depends on directly.
    pub fn roots(&self) -> impl Iterator<Item = &str> {
        self.roots.iter().map(String::as_str)
    }

    pub fn get(&self, name: &str) -> Option<&ResolvedPackage> {
        self.packages.get(name)
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// The `name@version` of every package, ordered by name. Useful for
    /// asserting on a plan and for reporting one.
    pub fn identifiers(&self) -> Vec<String> {
        self.packages.values().map(ResolvedPackage::id).collect()
    }
}
