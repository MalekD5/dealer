//! Turning a project's dependency ranges into a concrete install plan.
//!
//! dealer installs a single version of each package, so resolution is a search
//! for one version per name that satisfies every requirement collected along
//! the way. Requirements accumulate as the graph is walked; whenever a new one
//! invalidates the current choice the package is reconsidered, and packages
//! whose choice did not change are never expanded twice. That is what keeps
//! dependency cycles from looping forever.

pub mod plan;
pub mod provider;

pub use plan::InstallPlan;
pub use provider::{PackageProvider, RegistryProvider, VersionIndex};

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::error::{Error, Result, error};
use crate::package::{PackageSource, ResolvedPackage};
use crate::semver::select_version;

/// How requirements that come from the project itself are attributed.
const PROJECT: &str = "the project";

/// A ceiling on how much work one resolution may do, so a pathological graph
/// reports a failure instead of spinning.
const MAX_STEPS: usize = 100_000;

/// One range a package has to satisfy, and who asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Requirement {
    range: String,
    requested_by: String,
}

pub struct Resolver<'a> {
    provider: &'a dyn PackageProvider,
}

impl<'a> Resolver<'a> {
    pub fn new(provider: &'a dyn PackageProvider) -> Self {
        Self { provider }
    }

    /// Resolves direct and transitive dependencies into a deterministic plan.
    pub fn resolve(&self, roots: &[PackageSource]) -> Result<InstallPlan> {
        let mut state = Resolution::default();

        for source in roots {
            match source {
                PackageSource::LocalTarball { path, .. } => {
                    let package = self.provider.local_package(path)?;
                    state.roots.insert(package.name.clone());
                    state.pin(package)?;
                }
                PackageSource::Registry { name, range } => {
                    state.roots.insert(name.clone());
                    state.require(name, Requirement {
                        range: range.clone(),
                        requested_by: PROJECT.to_string(),
                    });
                }
            }
        }

        let mut steps = 0;
        while let Some(name) = state.pending.pop_front() {
            steps += 1;
            if steps > MAX_STEPS {
                return Err(error(format!(
                    "dependency resolution did not settle after {MAX_STEPS} steps; \
                     the graph may be pathologically large"
                )));
            }

            self.settle(&name, &mut state)?;
        }

        Ok(InstallPlan::new(state.selected, state.roots))
    }

    /// Chooses a version for one package, given everything asked of it so far.
    fn settle(&self, name: &str, state: &mut Resolution) -> Result<()> {
        let requirements = state.requirements.get(name).cloned().unwrap_or_default();

        // A local tarball fixes the version, so every range has to accept it.
        if let Some(pinned) = state.pinned.get(name).cloned() {
            for requirement in &requirements {
                if !range_admits(&requirement.range, &pinned.version)? {
                    return Err(conflict(name, &requirements, Some(&pinned.version)));
                }
            }

            return self.expand(pinned, state);
        }

        if requirements.is_empty() {
            return Ok(());
        }

        let index = self.provider.versions(name)?;
        if let Some(current) = state.selected.get(name) {
            let version = current.version.clone();
            let still_valid = requirements
                .iter()
                .try_fold(true, |valid, requirement| -> Result<bool> {
                    Ok(valid && admits(&index, &requirement.range, &version)?)
                })?;

            if still_valid {
                return Ok(());
            }
        }

        let version = self.choose(name, &index, &requirements)?;
        self.expand(self.provider.package(name, &version)?, state)
    }

    /// Records a package's choice and queues everything it depends on.
    fn expand(&self, package: ResolvedPackage, state: &mut Resolution) -> Result<()> {
        let requested_by = package.id();
        let dependencies = package.dependencies.clone();
        state.selected.insert(package.name.clone(), package);

        for (name, range) in dependencies {
            let source = PackageSource::parse(&name, &range).map_err(|reason| {
                error(format!("{requested_by} depends on `{name}`: {reason}"))
            })?;

            match source {
                PackageSource::Registry { name, range } => state.require(&name, Requirement {
                    range,
                    requested_by: requested_by.clone(),
                }),
                PackageSource::LocalTarball { path, .. } => {
                    state.pin(self.provider.local_package(&path)?)?;
                }
            }
        }

        Ok(())
    }

    /// The highest version that satisfies every requirement at once.
    fn choose(
        &self,
        name: &str,
        index: &VersionIndex,
        requirements: &[Requirement],
    ) -> Result<String> {
        let mut candidates: Option<Vec<String>> = None;

        for requirement in requirements {
            let matching = matching_versions(index, name, &requirement.range)?;
            candidates = Some(match candidates {
                // `matching` is ordered highest first, and intersecting keeps
                // that order, so the first survivor is the best choice.
                Some(current) => current
                    .into_iter()
                    .filter(|version| matching.contains(version))
                    .collect(),
                None => matching,
            });
        }

        candidates
            .and_then(|versions| versions.into_iter().next())
            .ok_or_else(|| conflict(name, requirements, None))
    }
}

/// Everything learned so far about the graph being resolved.
#[derive(Default)]
struct Resolution {
    selected: BTreeMap<String, ResolvedPackage>,
    requirements: BTreeMap<String, Vec<Requirement>>,
    /// Packages fixed by a local tarball, which no range may override.
    pinned: BTreeMap<String, ResolvedPackage>,
    roots: BTreeSet<String>,
    pending: VecDeque<String>,
}

impl Resolution {
    /// Adds a requirement, queueing the package only when the range is one it
    /// has not already been measured against. A second requester asking for a
    /// range that is already recorded cannot invalidate the current choice, so
    /// it is only worth remembering for the conflict report.
    fn require(&mut self, name: &str, requirement: Requirement) {
        let requirements = self.requirements.entry(name.to_string()).or_default();
        if requirements.contains(&requirement) {
            return;
        }

        let range_is_new = !requirements
            .iter()
            .any(|existing| existing.range == requirement.range);

        requirements.push(requirement);
        if range_is_new {
            self.pending.push_back(name.to_string());
        }
    }

    fn pin(&mut self, package: ResolvedPackage) -> Result<()> {
        if let Some(existing) = self.pinned.get(&package.name) {
            if existing.tarball != package.tarball {
                return Err(error(format!(
                    "`{}` is supplied by two different tarballs: {} and {}",
                    package.name, existing.tarball, package.tarball
                )));
            }

            return Ok(());
        }

        let name = package.name.clone();
        self.selected.insert(name.clone(), package.clone());
        self.pinned.insert(name.clone(), package);
        self.pending.push_back(name);

        Ok(())
    }
}

/// Versions satisfying a range, highest first. A dist-tag names exactly one.
fn matching_versions(index: &VersionIndex, name: &str, range: &str) -> Result<Vec<String>> {
    if let Some(version) = index.tags.get(range) {
        return Ok(vec![version.clone()]);
    }

    select_version(range.to_string(), index.versions.clone())
        .map_err(|failure| error(format!("`{range}` is not a valid range for `{name}`: {failure}")))
}

fn admits(index: &VersionIndex, range: &str, version: &str) -> Result<bool> {
    if let Some(tagged) = index.tags.get(range) {
        return Ok(tagged == version);
    }

    range_admits(range, version)
}

fn range_admits(range: &str, version: &str) -> Result<bool> {
    let matches = select_version(range.to_string(), vec![version.to_string()])
        .map_err(|failure| error(format!("`{range}` is not a valid range: {failure}")))?;

    Ok(!matches.is_empty())
}

fn conflict(name: &str, requirements: &[Requirement], pinned: Option<&str>) -> Error {
    let mut message = format!("cannot satisfy every requirement for `{name}`:");
    for requirement in requirements {
        message.push_str(&format!(
            "\n  {} is required by {}",
            requirement.range, requirement.requested_by
        ));
    }

    match pinned {
        Some(version) => message.push_str(&format!(
            "\n  but a local tarball pins `{name}` to {version}"
        )),
        None => message.push_str("\n  no published version satisfies all of them"),
    }

    error(message)
}
