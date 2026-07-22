//! Small filesystem helpers shared by the cache, store and linker.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, error};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Writes a file by way of a temporary sibling, so readers never observe a
/// half written cache entry.
pub fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| error(format!("{} has no parent directory", path.display())))?;
    create_directory(parent)?;

    let temporary = parent.join(temporary_name(path));
    fs::write(&temporary, contents).map_err(|failure| {
        error(format!(
            "could not write {}: {failure}",
            temporary.display()
        ))
    })?;

    if let Err(failure) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);

        // Losing the race is fine as long as somebody produced the file.
        if !path.exists() {
            return Err(error(format!(
                "could not move {} into place: {failure}",
                path.display()
            )));
        }
    }

    Ok(())
}

pub fn create_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .map_err(|failure| error(format!("could not create {}: {failure}", path.display())))
}

/// Removes a file, directory, or symlink, ignoring anything already gone.
pub fn remove_any(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(failure) => {
            return Err(error(format!(
                "could not inspect {}: {failure}",
                path.display()
            )));
        }
    };

    // A directory symlink must be unlinked rather than followed, and on Windows
    // that means `remove_dir` even though the link is not a real directory.
    let removal = if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };

    removal.or_else(|failure| match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(_) if !path.exists() => Ok(()),
        Err(_) => Err(failure),
    })
    .map_err(|failure| error(format!("could not remove {}: {failure}", path.display())))
}

/// A name no other in-flight write will choose.
pub fn temporary_name(path: &Path) -> PathBuf {
    let stem = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "entry".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);

    PathBuf::from(format!(
        ".{stem}.{}-{nanos}-{sequence}.tmp",
        std::process::id()
    ))
}
