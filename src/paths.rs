//! Locations dealer keeps its cache and package store in.

use std::env;
use std::path::PathBuf;

use crate::error::{Result, error};

/// The directory holding dealer's cache and store.
///
/// `DEALER_HOME` overrides the default, which keeps tests and sandboxes from
/// touching a developer's real cache.
pub fn home_directory() -> Result<PathBuf> {
    if let Some(overridden) = env::var_os("DEALER_HOME")
        && !overridden.is_empty()
    {
        return Ok(PathBuf::from(overridden));
    }

    #[cfg(windows)]
    let base = env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("dealer"));
    #[cfg(not(windows))]
    let base = env::var_os("HOME").map(|base| PathBuf::from(base).join(".dealer"));

    base.ok_or_else(|| {
        error("could not determine a home directory; set `DEALER_HOME` to choose one")
    })
}

/// Where downloaded tarballs are kept.
pub fn cache_directory() -> Result<PathBuf> {
    Ok(home_directory()?.join("cache"))
}

/// Where verified tarballs are extracted to.
pub fn store_directory() -> Result<PathBuf> {
    Ok(home_directory()?.join("store"))
}
