//! Package tarball download and extraction support.

pub mod archive;
pub mod cache;
pub mod extract;

pub use archive::{ArchiveReader, Entry, EntryKind};
pub use cache::{Artifact, TarballCache};
pub use extract::extract_package;

use serde_json::Value;

use crate::error::{Result, error};

/// Reads the `package.json` at the root of a gzipped npm tarball without
/// extracting anything to disk.
pub fn read_manifest(compressed: &[u8]) -> Result<Value> {
    let archive = extract::decompress(compressed)?;
    let mut reader = ArchiveReader::new(&archive);

    while let Some(entry) = reader.next_entry()? {
        // npm roots archives at a single directory, so the manifest sits at the
        // second component of its path.
        if entry.kind != EntryKind::File || !is_root_manifest(&entry.path) {
            continue;
        }

        return serde_json::from_slice(entry.data).map_err(|failure| {
            error(format!(
                "the tarball's package.json is not valid JSON: {failure}"
            ))
        });
    }

    Err(error("the tarball does not contain a package.json"))
}

fn is_root_manifest(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let mut components = normalized
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".");

    components.next().is_some()
        && components.next() == Some("package.json")
        && components.next().is_none()
}
