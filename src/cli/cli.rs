use clap::{Parser, Subcommand};

use super::{
    init,
    run::{self, RunArgs},
};
use crate::manifest::manifest::PackageManifest;

pub type CommandResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(version, about = "Dealer package manager CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a package.json in the current directory
    Init,
    Run {
        script_name: String,
    },
}

pub fn read_input() -> CommandResult {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => init::init(),
        Command::Run { script_name } => {
            let project_directory = std::env::current_dir()?;
            let mut package_manifest =
                PackageManifest::new(project_directory.to_string_lossy().into_owned())
                    .map_err(std::io::Error::other)?;

            run::run(RunArgs { script_name }, &mut package_manifest)
        }
    }
}
