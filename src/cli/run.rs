use super::cli::CommandResult;
use crate::manifest::manifest::PackageManifest;
use clap::Args;
use std::process::{Command, Stdio};

#[derive(Debug, Args)]
pub struct RunArgs {
    pub script_name: String,
}

pub fn run(args: RunArgs, context: &mut PackageManifest) -> CommandResult {
    let script_name = args.script_name;

    let script = context
        .scripts
        .get(&script_name)
        .ok_or_else(|| format!("script `{script_name}` was not found"))?;

    println!("> {}@{} {script_name}", context.name, context.version);
    println!("> {script}");

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", script]);
        command
    };

    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    };

    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err(format!("script `{script_name}` exited with {status}").into());
    }

    Ok(())
}
