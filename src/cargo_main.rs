//! `cargo coderipper`: cargo finds this binary on PATH as the subcommand `coderipper`.

use std::process::ExitCode;

fn main() -> ExitCode {
    coderipper::cli::run_as_cargo_subcommand(std::env::args_os().collect())
}
