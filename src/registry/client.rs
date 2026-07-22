use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::rc::Rc;
use std::time::Duration;

use serde_json::Value;

use super::packument::Packument;
use crate::error::{Result, error};

const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

/// Asks npm for the compact packument, which omits fields dealer never reads.
const ABBREVIATED_METADATA: &str = "application/vnd.npm.install-v1+json, application/json";

/// Registry documents and tarballs are both read fully into memory, so both
/// need a ceiling.
const MAX_METADATA_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TARBALL_BYTES: u64 = 256 * 1024 * 1024;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Reads package metadata and tarballs over HTTP.
pub struct RegistryClient {
    base_url: String,
    agent: ureq::Agent,
    packuments: RefCell<HashMap<String, Rc<Packument>>>,
}

impl RegistryClient {
    /// Uses `DEALER_REGISTRY`, then `npm_config_registry`, then npm itself.
    pub fn new() -> Self {
        Self::with_base_url(configured_registry())
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!("dealer/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();

        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            agent: ureq::Agent::new_with_config(config),
            packuments: RefCell::new(HashMap::new()),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Fetches the metadata document for `name`, reusing it across calls.
    pub fn packument(&self, name: &str) -> Result<Rc<Packument>> {
        if let Some(cached) = self.packuments.borrow().get(name) {
            return Ok(Rc::clone(cached));
        }

        let url = self.package_url(name);
        let body = self
            .agent
            .get(&url)
            .header("accept", ABBREVIATED_METADATA)
            .call()
            .map_err(|failure| self.describe_metadata_failure(name, &url, failure))?
            .body_mut()
            .with_config()
            .limit(MAX_METADATA_BYTES)
            .read_to_vec()
            .map_err(|failure| error(format!("could not read metadata for `{name}`: {failure}")))?;

        let document: Value = serde_json::from_slice(&body).map_err(|failure| {
            error(format!(
                "registry returned metadata for `{name}` that is not valid JSON: {failure}"
            ))
        })?;

        let packument = Rc::new(Packument::parse(name, &document)?);
        self.packuments
            .borrow_mut()
            .insert(name.to_string(), Rc::clone(&packument));

        Ok(packument)
    }

    /// Downloads a tarball into memory.
    pub fn download(&self, url: &str) -> Result<Vec<u8>> {
        self.agent
            .get(url)
            .call()
            .map_err(|failure| match failure {
                ureq::Error::StatusCode(status) => {
                    error(format!("downloading {url} failed with HTTP {status}"))
                }
                failure => error(format!("could not download {url}: {failure}")),
            })?
            .body_mut()
            .with_config()
            .limit(MAX_TARBALL_BYTES)
            .read_to_vec()
            .map_err(|failure| error(format!("could not read the tarball at {url}: {failure}")))
    }

    /// Scoped names carry a `/` that has to survive as a single path segment.
    fn package_url(&self, name: &str) -> String {
        format!("{}/{}", self.base_url, name.replace('/', "%2F"))
    }

    fn describe_metadata_failure(
        &self,
        name: &str,
        url: &str,
        failure: ureq::Error,
    ) -> crate::error::Error {
        let registry = &self.base_url;
        match failure {
            ureq::Error::StatusCode(404) => {
                error(format!("package `{name}` was not found in {registry}"))
            }
            ureq::Error::StatusCode(status @ (401 | 403)) => error(format!(
                "{registry} refused access to `{name}` (HTTP {status}); \
                 the package may be private or require authentication"
            )),
            ureq::Error::StatusCode(429) => error(format!(
                "{registry} is rate limiting this client (HTTP 429); retry in a moment"
            )),
            ureq::Error::StatusCode(status) if status >= 500 => error(format!(
                "{registry} failed to serve `{name}` (HTTP {status}); the registry may be down"
            )),
            ureq::Error::StatusCode(status) => error(format!("{url} responded with HTTP {status}")),
            ureq::Error::HostNotFound => error(format!(
                "could not resolve the host for {registry}; check the registry URL and your network"
            )),
            ureq::Error::Timeout(_) => error(format!(
                "{registry} did not answer within {} seconds",
                REQUEST_TIMEOUT.as_secs()
            )),
            failure => error(format!("could not reach {registry}: {failure}")),
        }
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

fn configured_registry() -> String {
    for variable in ["DEALER_REGISTRY", "npm_config_registry"] {
        if let Ok(value) = env::var(variable) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }

    DEFAULT_REGISTRY.to_string()
}
