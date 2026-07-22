use std::process::ExitCode;

fn main() -> ExitCode {
    match dealer::cli::read_input() {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("error: {failure}");
            ExitCode::FAILURE
        }
    }
}
