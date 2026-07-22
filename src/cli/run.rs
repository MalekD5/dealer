use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use clap::Args;

use super::cli::CommandResult;
use crate::error::error;
use crate::linker::bin::BIN_DIRECTORY;
use crate::manifest::manifest::PackageManifest;

#[derive(Debug, Args)]
pub struct RunArgs {
    pub script_name: String,

    /// Arguments appended to the script, usually written after `--`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub arguments: Vec<String>,
}

pub fn run(args: RunArgs, context: &PackageManifest) -> CommandResult {
    let script_name = args.script_name;

    let script = context
        .scripts
        .get(&script_name)
        .ok_or_else(|| error(format!("script `{script_name}` was not found")))?;
    let command_line = append_arguments(script, &args.arguments);

    println!("> {}@{} {script_name}", context.name, context.version);
    println!("> {command_line}");

    let status = shell_command(&command_line)
        .current_dir(&context.directory)
        .env("PATH", script_path(&context.directory))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::inherit())
        .status()?;

    if status.success() {
        return Ok(ExitCode::SUCCESS);
    }

    match status.code() {
        Some(code) => {
            eprintln!("script `{script_name}` exited with code {code}");
            Ok(ExitCode::from(exit_byte(code)))
        }
        // A signal leaves no exit code to hand back, so report the status.
        None => Err(error(format!(
            "script `{script_name}` exited with {status}"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn shell_command(command_line: &str) -> Command {
    use std::os::windows::process::CommandExt;

    // `cmd` parses the raw command line rather than the argv the standard
    // escaping is built for, which turns a script's own quotes into
    // backslash-escaped literals. Appending the line verbatim is the only way
    // a script like `node -e "..."` reaches the shell as it was written.
    let mut command = Command::new("cmd");
    command.raw_arg("/C").raw_arg(command_line);
    command
}

#[cfg(not(target_os = "windows"))]
fn shell_command(command_line: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", command_line]);
    command
}

/// Appends forwarded arguments to the script, quoting each one so the shell
/// sees exactly what the caller typed.
fn append_arguments(script: &str, arguments: &[String]) -> String {
    // clap hands back the `--` separator itself when it leads the list.
    let arguments = match arguments.split_first() {
        Some((first, rest)) if first == "--" => rest,
        _ => arguments,
    };

    arguments
        .iter()
        .fold(script.to_string(), |mut line, argument| {
            line.push(' ');
            line.push_str(&quote(argument));
            line
        })
}

#[cfg(target_os = "windows")]
fn quote(argument: &str) -> String {
    format!("\"{}\"", argument.replace('"', "\"\""))
}

#[cfg(not(target_os = "windows"))]
fn quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', r"'\''"))
}

/// PATH for a script, with the `.bin` directory of this project and of every
/// ancestor ahead of everything else.
///
/// Walking up matters inside workspaces, where a dependency may have been
/// hoisted into a parent project's `node_modules`.
fn script_path(project: &Path) -> OsString {
    let mut directories: Vec<PathBuf> = project
        .ancestors()
        .map(|ancestor| ancestor.join("node_modules").join(BIN_DIRECTORY))
        .collect();

    if let Some(existing) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&existing));
    }

    std::env::join_paths(directories)
        .unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

/// Exit codes reach the parent process as a byte, and a code whose low byte is
/// zero must not be reported as success.
fn exit_byte(code: i32) -> u8 {
    match code as u8 {
        0 => 1,
        byte => byte,
    }
}
