use std::process::ExitCode;

fn main() -> ExitCode {
    coderipper::cli::run(std::env::args_os().collect())
}
