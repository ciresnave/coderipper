use clap::{Parser, Subcommand};
use coderipper::check::{CheckContext, Tier};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "coderipper",
    about = "Portfolio-wide code-integration auditor — run it next to cargo clippy, or as a server."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run every fast-tier check (local-only: no registry or GitHub API calls). Meant for CI and
    /// on-demand use next to `cargo clippy`.
    Fast {
        /// Project to check. Defaults to the current directory.
        #[arg(long)]
        project: Option<PathBuf>,
        /// Root of the portfolio, for Portfolio-scope checks. Defaults to the parent of `project`.
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Run every check, including network-required ones (registries, GitHub API). Meant for a
    /// periodic or PM-triggered pass, not a per-PR gate.
    Sweep {
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Run exactly one check by id, at any tier.
    Check {
        id: String,
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Run as an HTTP server (not yet implemented — planned for the ThinkersJournal.com hosted
    /// instance; see docs/superpowers/specs/2026-09-30-audit-host-design.md §7).
    Serve {
        #[arg(long, default_value = "8080")]
        port: u16,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fast {
            project,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Fast, None),
        Command::Sweep {
            project,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Sweep, None),
        Command::Check {
            id,
            project,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Fast, Some(id)),
        Command::Serve { port } => {
            anyhow::bail!(
                "server mode is not implemented yet (port {port} requested) — see design doc §7"
            )
        }
    }
}

fn run_and_report(
    project: Option<PathBuf>,
    portfolio_root: Option<PathBuf>,
    tier: Tier,
    only_check_id: Option<String>,
) -> anyhow::Result<()> {
    let project_root = project.unwrap_or(std::env::current_dir()?).canonicalize()?;
    let portfolio_root = portfolio_root
        .or_else(|| project_root.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project_root.clone());

    let ctx = CheckContext {
        project_root,
        portfolio_root,
    };

    let result = coderipper::run_checks(&ctx, tier, only_check_id.as_deref());

    // Review finding: this used to print "no checks registered yet" whenever BOTH findings and
    // errors were empty -- which is what a genuinely clean run also looks like, since a check IS
    // always registered (this message predates ReachabilityCheck being wired in and was never
    // updated). Distinguish the two real cases instead.
    if result.findings.is_empty() && result.errors.is_empty() {
        println!("coderipper: no issues found");
        return Ok(());
    }

    for f in &result.findings {
        println!(
            "[{:?}/{:?}] {} — {} ({})",
            f.severity, f.confidence, f.project, f.summary, f.check_id
        );
    }
    for e in &result.errors {
        eprintln!("coderipper: check error: {e}");
    }

    // Review finding: errors were printed to stderr but `main` still returned `Ok(())`, so the
    // process exited 0 even when a check genuinely failed to run -- a CI pipeline gating on exit
    // code would see success. A check error must fail the run.
    anyhow::ensure!(
        result.errors.is_empty(),
        "{} error(s) during the run (see above)",
        result.errors.len()
    );

    Ok(())
}
