mod cli;
mod manifest;

use cli::CommandResult;
use manifest::manifest::PackageManifest;

fn main() -> CommandResult {
    let project_directory = std::env::current_dir()?;
    let package_manifest = PackageManifest::new(project_directory.to_string_lossy().into_owned())
        .map_err(std::io::Error::other)?;

    cli::read_input(package_manifest)
}
