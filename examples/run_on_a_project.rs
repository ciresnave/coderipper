//! Runs the fast built-in checks on a project and prints what they report.
//!
//! ```text
//! cargo run --example run_on_a_project -- path/to/project
//! ```
//!
//! The project must be a git repository with a commit: the checks analyse `HEAD` in a throwaway worktree and never
//! touch your working tree. With no argument it checks the current directory.

use coderipper::check::{CheckContext, Tier};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let project = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let project = match project.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("cannot read {}: {e}", project.display());
            return ExitCode::from(2);
        }
    };
    let portfolio = project.parent().unwrap_or(&project).to_path_buf();

    let ctx = CheckContext::new(project, portfolio);
    let result = coderipper::run_checks(&ctx, Tier::Fast, None);

    for finding in &result.findings {
        println!(
            "{:?} {} ({})",
            finding.severity, finding.summary, finding.check_id
        );
    }
    if result.findings.is_empty() && result.errors.is_empty() {
        println!("no issues found");
    }
    // A check that could not run is reported here, never as an absence of findings.
    for error in &result.errors {
        eprintln!("check error: {error}");
    }
    if result.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}
