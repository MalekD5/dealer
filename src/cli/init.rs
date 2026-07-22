use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::process::ExitCode;

use super::cli::CommandResult;
use crate::manifest::json_loader::MANIFEST_FILE;
use crate::manifest::package_name::normalize_package_name;

pub fn init() -> CommandResult {
    let directory = std::env::current_dir()?;
    let path = directory.join(MANIFEST_FILE);

    let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            println!("package.json already found");
            return Ok(ExitCode::SUCCESS);
        }
        Err(error) => return Err(error.into()),
    };

    let directory_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package");
    let name = normalize_package_name(directory_name);
    let manifest = serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "scripts": {}
    });

    serde_json::to_writer_pretty(&mut file, &manifest)?;
    writeln!(file)?;
    println!("created package.json");

    Ok(ExitCode::SUCCESS)
}
