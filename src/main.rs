mod cli;
mod manifest;

use cli::CommandResult;

fn main() -> CommandResult {
    cli::read_input()
}
