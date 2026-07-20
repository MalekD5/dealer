use crate::manifest::manifest::PackageManifest;
use clap::{Parser, Subcommand};

use super::run::{self, RunArgs};

pub type CommandResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(version, about = "Dealer package manager CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run { script_name: String },
}

pub fn read_input(mut package_manifest: PackageManifest) -> CommandResult {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { script_name } => run::run(RunArgs { script_name }, &mut package_manifest),
    }
}
