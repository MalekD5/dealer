//! The `dealer` command line interface.

pub mod cli;
pub mod init;
pub mod install;
pub mod run;

pub use cli::{CommandResult, read_input};
