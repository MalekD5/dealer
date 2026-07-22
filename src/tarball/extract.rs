//! Safe extraction of package tarballs.
//!
//! Archives arrive from the network, so every path and link is validated
//! against the destination directory before anything touches the filesystem.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;

use super::archive::{ArchiveReader, Entry, EntryKind};
use crate::error::{Result, error};
use crate::util::{clone_file, clone_tree, has_drive_prefix, safe_components};

/// Ceilings that keep a hostile archive from filling the disk.
const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

/// Inflates a gzip stream, refusing archives that expand implausibly.
pub fn decompress(compressed: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(compressed).take(MAX_DECOMPRESSED_BYTES + 1);
    let mut data = Vec::new();

    decoder
        .read_to_end(&mut data)
        .map_err(|failure| error(format!("could not decompress the tarball: {failure}")))?;

    if data.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(error(format!(
            "tarball expands beyond the {MAX_DECOMPRESSED_BYTES} byte extraction limit"
        )));
    }

    Ok(data)
}

/// Extracts a gzipped npm tarball into `destination`.
///
/// npm roots every archive at a single directory, conventionally `package/`;
/// that component is stripped so `destination` becomes the package root.
pub fn extract_package(compressed: &[u8], destination: &Path) -> Result<()> {
    let archive = decompress(compressed)?;
    let mut reader = ArchiveReader::new(&archive);
    let mut deferred_links = Vec::new();
    let mut count = 0;

    fs::create_dir_all(destination).map_err(|failure| {
        error(format!(
            "could not create {}: {failure}",
            destination.display()
        ))
    })?;

    while let Some(entry) = reader.next_entry()? {
        count += 1;
        if count > MAX_ENTRIES {
            return Err(error(format!(
                "tarball contains more than {MAX_ENTRIES} entries"
            )));
        }

        let Some(relative) = strip_archive_root(&entry.path)? else {
            continue;
        };
        let target = destination.join(&relative);

        match entry.kind {
            EntryKind::Directory => create_directory(&target)?,
            EntryKind::File => write_file(&entry, &target)?,
            EntryKind::Symlink | EntryKind::HardLink => {
                deferred_links.push(prepare_link(&entry, &relative)?);
            }
            EntryKind::Unsupported(flag) => {
                return Err(error(format!(
                    "`{}` is a type `{}` entry; packages may only contain files, \
                     directories and links",
                    entry.path,
                    char::from(flag).escape_default()
                )));
            }
        }
    }

    // Links are materialised last so their targets have already been written.
    for link in deferred_links {
        materialize_link(&link, destination)?;
    }

    Ok(())
}

/// A validated link waiting for its target to exist.
struct PendingLink {
    /// Path of the link itself, relative to the package root.
    path: PathBuf,
    /// Path of the target, relative to the package root.
    target: PathBuf,
    /// The literal target recorded in the archive, used for Unix symlinks.
    literal_target: String,
    symbolic: bool,
}

fn create_directory(target: &Path) -> Result<()> {
    fs::create_dir_all(target)
        .map_err(|failure| error(format!("could not create {}: {failure}", target.display())))
}

fn write_file(entry: &Entry<'_>, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        create_directory(parent)?;
    }

    fs::write(target, entry.data)
        .map_err(|failure| error(format!("could not write {}: {failure}", target.display())))?;

    #[cfg(unix)]
    if entry.is_executable() {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(target, fs::Permissions::from_mode(0o755)).map_err(|failure| {
            error(format!(
                "could not mark {} executable: {failure}",
                target.display()
            ))
        })?;
    }

    Ok(())
}

fn prepare_link(entry: &Entry<'_>, relative: &Path) -> Result<PendingLink> {
    let literal_target = entry
        .link_target
        .clone()
        .ok_or_else(|| error(format!("`{}` is a link with no target", entry.path)))?;
    let symbolic = entry.kind == EntryKind::Symlink;

    // A symlink is read relative to its own directory; a hard link names
    // another member of the archive.
    let target = if symbolic {
        let base = relative.parent().unwrap_or(Path::new(""));
        resolve_within_root(base, &literal_target).ok_or_else(|| {
            error(format!(
                "`{}` links to `{literal_target}`, which escapes the package",
                entry.path
            ))
        })?
    } else {
        strip_archive_root(&literal_target)?.ok_or_else(|| {
            error(format!(
                "`{}` hard links to `{literal_target}`, which is outside the package",
                entry.path
            ))
        })?
    };

    Ok(PendingLink {
        path: relative.to_path_buf(),
        target,
        literal_target,
        symbolic,
    })
}

fn materialize_link(link: &PendingLink, destination: &Path) -> Result<()> {
    let path = destination.join(&link.path);
    let target = destination.join(&link.target);

    if let Some(parent) = path.parent() {
        create_directory(parent)?;
    }

    // Unix would happily create a link to nothing, but a package that installs
    // there and fails on Windows is worse than one rejected on both, so the
    // target is checked before any platform gets involved.
    if !target.exists() {
        let kind = if link.symbolic {
            "symlink"
        } else {
            "hard link"
        };

        return Err(error(format!(
            "`{}` is a {kind} to `{}`, which the package does not contain",
            link.path.display(),
            link.literal_target
        )));
    }

    #[cfg(unix)]
    if link.symbolic {
        return std::os::unix::fs::symlink(&link.literal_target, &path).map_err(|failure| {
            error(format!(
                "could not link {} to {}: {failure}",
                path.display(),
                link.literal_target
            ))
        });
    }

    // Hard links, and every link on Windows, are materialised as copies so the
    // extracted tree stays self contained and needs no elevated privileges.
    if target.is_dir() {
        return clone_tree(&target, &path);
    }

    clone_file(&target, &path)
}

/// Validates an archive path and removes the single directory npm roots
/// packages at.
///
/// Returns `Ok(None)` for entries that are only that root directory.
fn strip_archive_root(raw: &str) -> Result<Option<PathBuf>> {
    let components =
        safe_components(raw).map_err(|failure| error(format!("archive entry {failure}")))?;

    if components.len() < 2 {
        return Ok(None);
    }

    Ok(Some(components[1..].iter().collect()))
}

/// Resolves a relative link target lexically and rejects targets that climb
/// out of the package root.
fn resolve_within_root(base: &Path, target: &str) -> Option<PathBuf> {
    if target.contains('\0') {
        return None;
    }

    let normalized = target.replace('\\', "/");
    if normalized.starts_with('/') || has_drive_prefix(&normalized) {
        return None;
    }

    let mut resolved: Vec<String> = base
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();

    for component in normalized.split('/') {
        match component {
            "" | "." => continue,
            ".." => {
                resolved.pop()?;
            }
            component => resolved.push(component.to_string()),
        }
    }

    if resolved.is_empty() {
        return None;
    }

    Some(resolved.iter().collect())
}
