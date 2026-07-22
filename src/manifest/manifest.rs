use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use serde_json::Value;

use crate::error::{Result, error};
use crate::manifest::json_loader::{load_document, manifest_path};
use crate::manifest::package_name::validate_package_name;
use crate::package::{PackageSource, read_dependencies};
use crate::util::write_atomically;

pub struct PackageManifest {
    /// The project directory the manifest was read from.
    pub directory: PathBuf,
    pub name: String,
    pub version: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub scripts: HashMap<String, String>,
    pub dependencies: BTreeMap<String, String>,
    pub dev_dependencies: BTreeMap<String, String>,
    /// The manifest exactly as parsed, so saving preserves the fields dealer
    /// does not model.
    document: Value,
}

impl PackageManifest {
    pub fn load(directory: impl Into<PathBuf>) -> Result<PackageManifest> {
        let directory = directory.into();
        let document = load_document(&directory)?;

        let name = document
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| error("`name` is missing or is not a string"))?
            .to_string();
        validate_package_name(&name)
            .map_err(|failure| error(format!("invalid package name: {failure}")))?;

        let version = document
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| error("`version` is missing or is not a string"))?
            .to_string();

        let description = document
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);

        let scripts = match document.get("scripts") {
            Some(value) => serde_json::from_value::<HashMap<String, String>>(value.clone())
                .map_err(|failure| {
                    error(format!(
                        "`scripts` must contain only string values: {failure}"
                    ))
                })?,
            None => HashMap::new(),
        };

        Ok(PackageManifest {
            dependencies: read_dependencies(&document, "dependencies"),
            dev_dependencies: read_dependencies(&document, "devDependencies"),
            directory,
            name,
            version,
            description,
            scripts,
            document,
        })
    }

    pub fn path(&self) -> PathBuf {
        manifest_path(&self.directory)
    }

    /// Where this project keeps its installed packages.
    pub fn node_modules(&self) -> PathBuf {
        self.directory.join("node_modules")
    }

    /// The dependencies an install starts from.
    ///
    /// Development dependencies belong to the project being worked on rather
    /// than to anything consuming it, so they only apply at the root.
    pub fn install_sources(&self, include_development: bool) -> Result<Vec<PackageSource>> {
        let development = include_development
            .then_some(&self.dev_dependencies)
            .into_iter()
            .flatten();

        self.dependencies
            .iter()
            .chain(development)
            .map(|(name, range)| {
                PackageSource::parse(name, range).map_err(|reason| {
                    error(format!("`{name}` has an unusable specifier: {reason}"))
                })
            })
            .collect()
    }

    /// Records a dependency, updating the table it already appears in so a
    /// package never lands in two of them.
    pub fn set_dependency(&mut self, name: &str, specifier: &str) {
        let development = self.dev_dependencies.contains_key(name);
        let table = if development {
            &mut self.dev_dependencies
        } else {
            &mut self.dependencies
        };
        table.insert(name.to_string(), specifier.to_string());

        let key = if development {
            "devDependencies"
        } else {
            "dependencies"
        };
        if let Some(document) = self.document.as_object_mut() {
            let entries = document
                .entry(key)
                .or_insert_with(|| Value::Object(serde_json::Map::new()));

            if let Some(entries) = entries.as_object_mut() {
                entries.insert(name.to_string(), Value::String(specifier.to_string()));
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = self.path();
        let serialized = serde_json::to_string_pretty(&self.document).map_err(|failure| {
            error(format!("could not serialize {}: {failure}", path.display()))
        })?;

        write_atomically(&path, format!("{serialized}\n").as_bytes())
    }
}
