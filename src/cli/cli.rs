use std::process::ExitCode;

use clap::{Parser, Subcommand};

use super::{
    init,
    install::{self, InstallArgs},
    run::{self, RunArgs},
};
use crate::manifest::manifest::PackageManifest;

pub type CommandResult = crate::error::Result<ExitCode>;

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

    /// Install the project's dependencies into node_modules
    Install(InstallArgs),

    /// Run a script declared in package.json
    Run(RunArgs),
}

pub fn read_input() -> CommandResult {
    let cli = Cli::parse();
    let project = std::env::current_dir()?;

    match cli.command {
        Command::Init => init::init(),
        Command::Install(args) => install::install(args, &project),
        Command::Run(args) => run::run(args, &PackageManifest::load(project)?),
    }
}
