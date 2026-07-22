use std::fmt;
use std::path::{Path, PathBuf};

use crate::manifest::package_name::validate_package_name;

/// Where a requested dependency comes from, before it is resolved to a
/// concrete version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageSource {
    /// A package published to a registry, narrowed by a semver range.
    Registry { name: String, range: String },
    /// A `.tgz` archive that already exists on disk.
    LocalTarball { name: Option<String>, path: PathBuf },
}

impl PackageSource {
    /// Interprets a `package.json` dependency entry.
    ///
    /// `file:` specifiers and anything that looks like a path to a tarball
    /// resolve locally; everything else is treated as a registry range.
    pub fn parse(name: &str, specifier: &str) -> Result<Self, String> {
        validate_package_name(name)?;

        let specifier = specifier.trim();
        if let Some(path) = local_tarball_path(specifier) {
            return Ok(PackageSource::LocalTarball {
                name: Some(name.to_string()),
                path,
            });
        }

        Ok(PackageSource::Registry {
            name: name.to_string(),
            range: normalize_range(specifier),
        })
    }

    /// Interprets a command line specifier such as `lodash@^4.17.0`,
    /// `@scope/pkg`, or `./fixture.tgz`.
    pub fn parse_specifier(specifier: &str) -> Result<Self, String> {
        let specifier = specifier.trim();
        if let Some(path) = local_tarball_path(specifier) {
            return Ok(PackageSource::LocalTarball { name: None, path });
        }

        // A leading `@` belongs to the scope, so only look for a range
        // separator after it.
        let scope_length = usize::from(specifier.starts_with('@'));
        let (name, range) = match specifier[scope_length..].split_once('@') {
            Some((name, range)) => (&specifier[..scope_length + name.len()], range),
            None => (specifier, ""),
        };

        PackageSource::parse(name, range)
    }

    /// The package name, when it is known before resolution.
    pub fn name(&self) -> Option<&str> {
        match self {
            PackageSource::Registry { name, .. } => Some(name),
            PackageSource::LocalTarball { name, .. } => name.as_deref(),
        }
    }
}

impl fmt::Display for PackageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageSource::Registry { name, range } => write!(formatter, "{name}@{range}"),
            PackageSource::LocalTarball { name, path } => match name {
                Some(name) => write!(formatter, "{name} (file:{})", path.display()),
                None => write!(formatter, "file:{}", path.display()),
            },
        }
    }
}

/// npm treats an empty specifier, `*`, and the `latest` tag as "any version".
fn normalize_range(specifier: &str) -> String {
    if specifier.is_empty() || specifier == "latest" {
        return "*".to_string();
    }

    specifier.to_string()
}

fn local_tarball_path(specifier: &str) -> Option<PathBuf> {
    let path = specifier.strip_prefix("file:").unwrap_or(specifier);
    let looks_like_path = specifier.starts_with("file:")
        || path.starts_with("./")
        || path.starts_with("../")
        || path.starts_with(".\\")
        || path.starts_with("..\\")
        || Path::new(path).is_absolute();

    if !looks_like_path {
        return None;
    }
    if !is_tarball(path) {
        return None;
    }

    Some(PathBuf::from(path))
}

fn is_tarball(path: &str) -> bool {
    let lowercase = path.to_ascii_lowercase();
    lowercase.ends_with(".tgz") || lowercase.ends_with(".tar.gz")
}
