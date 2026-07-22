//! Command shims in `node_modules/.bin`.
//!
//! A shim is a tiny launcher that runs a dependency's entry script with node.
//! Because they all live in one directory, putting that directory on PATH is
//! enough for package scripts to find every dependency's executables.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Result, error};
use crate::util::{create_directory, remove_any};

/// Where shims live, relative to `node_modules`.
pub const BIN_DIRECTORY: &str = ".bin";

pub fn bin_directory(node_modules: &Path) -> PathBuf {
    node_modules.join(BIN_DIRECTORY)
}

/// Writes the shims for one executable.
///
/// `target` is the path of the script relative to `node_modules`, for example
/// `left-pad/cli.js`.
pub fn write_shim(node_modules: &Path, name: &str, target: &str) -> Result<()> {
    let directory = bin_directory(node_modules);
    create_directory(&directory)?;

    // Shims sit one level below `node_modules`, so every target is reached by
    // stepping back out of `.bin`.
    let relative = format!("../{target}");
    let shim = directory.join(name);

    write_executable(&shim, &posix_shim(&relative))?;

    if cfg!(windows) {
        write_executable(&shim.with_extension("cmd"), &command_shim(&relative))?;
        write_executable(&shim.with_extension("ps1"), &power_shell_shim(&relative))?;
    }

    Ok(())
}

/// Removes every shim written for `name`.
pub fn remove_shim(node_modules: &Path, name: &str) -> Result<()> {
    let shim = bin_directory(node_modules).join(name);

    remove_any(&shim)?;
    remove_any(&shim.with_extension("cmd"))?;
    remove_any(&shim.with_extension("ps1"))?;

    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> Result<()> {
    // A shim may be replacing one that is currently being executed, so it is
    // removed rather than truncated.
    remove_any(path)?;
    fs::write(path, contents)
        .map_err(|failure| error(format!("could not write {}: {failure}", path.display())))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|failure| {
            error(format!(
                "could not mark {} executable: {failure}",
                path.display()
            ))
        })?;
    }

    Ok(())
}

fn posix_shim(relative: &str) -> String {
    format!(
        "#!/bin/sh\n\
         basedir=$(dirname \"$0\")\n\
         exec node \"$basedir/{relative}\" \"$@\"\n"
    )
}

fn command_shim(relative: &str) -> String {
    let relative = relative.replace('/', "\\");

    format!(
        "@ECHO off\r\n\
         SETLOCAL\r\n\
         SET \"NODE_EXE=%~dp0\\node.exe\"\r\n\
         IF NOT EXIST \"%NODE_EXE%\" SET \"NODE_EXE=node\"\r\n\
         \"%NODE_EXE%\" \"%~dp0\\{relative}\" %*\r\n\
         EXIT /b %ERRORLEVEL%\r\n"
    )
}

fn power_shell_shim(relative: &str) -> String {
    format!(
        "#!/usr/bin/env pwsh\n\
         $basedir = Split-Path $MyInvocation.MyCommand.Definition -Parent\n\
         & node \"$basedir/{relative}\" $args\n\
         exit $LASTEXITCODE\n"
    )
}
