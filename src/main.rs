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
        /// Check every member of the cargo workspace containing the project, not just the project.
        #[arg(long)]
        workspace: bool,
        /// Root of the portfolio, for Portfolio-scope checks. Defaults to the parent of `project`.
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Run every check, including network-required ones (registries, GitHub API). Meant for a
    /// periodic or PM-triggered pass, not a per-PR gate.
    Sweep {
        #[arg(long)]
        project: Option<PathBuf>,
        /// Check every member of the cargo workspace containing the project, not just the project.
        #[arg(long)]
        workspace: bool,
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Run exactly one check by id, at any tier.
    Check {
        id: String,
        #[arg(long)]
        project: Option<PathBuf>,
        /// Check every member of the cargo workspace containing the project, not just the project.
        #[arg(long)]
        workspace: bool,
        #[arg(long)]
        portfolio_root: Option<PathBuf>,
    },
    /// Inspect or trim the build cache (the persistent target directory the checks build in).
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Run as an HTTP server (not yet implemented — planned for the ThinkersJournal.com hosted
    /// instance; see docs/superpowers/specs/2026-09-30-audit-host-design.md §7).
    Serve {
        #[arg(long, default_value = "8080")]
        port: u16,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// List the cache's repository directories with their sizes.
    Status,
    /// Delete least-recently-used repository directories until the cache fits (default: the configured cap).
    Prune {
        /// Cap in GB for this run, instead of `CODERIPPER_CACHE_MAX_GB` (20).
        #[arg(long)]
        max_gb: Option<f64>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let from_env = coderipper::build_cache::config_from_env(&|k| std::env::var(k).ok());
    if let Some(config) = from_env.clone() {
        coderipper::build_cache::set_cache_config(config);
    }

    let result = dispatch(cli, from_env);
    // What the builds did, once, whichever way the run ended.
    if let Some(text) =
        coderipper::build_cache::render_stats(&coderipper::build_cache::take_stats())
    {
        eprintln!("{text}");
    }
    result
}

fn cache_command(
    action: CacheAction,
    config: Option<coderipper::build_cache::CacheConfig>,
) -> anyhow::Result<()> {
    use coderipper::build_cache::{prune_to_cap, status};
    let config = config.ok_or_else(|| {
        anyhow::anyhow!(
            "the build cache is off (CODERIPPER_CACHE=off, or no cache directory could be derived)"
        )
    })?;
    match action {
        CacheAction::Status => {
            let dirs = status(&config.root)?;
            if dirs.is_empty() {
                println!("(empty)");
            }
            for dir in dirs {
                let ago = std::time::SystemTime::now()
                    .duration_since(dir.last_used)
                    .map_or(0, |d| d.as_secs());
                println!(
                    "{} bytes  last used {ago}s ago  {}",
                    dir.bytes,
                    dir.path.display()
                );
            }
        }
        CacheAction::Prune { max_gb } => {
            let cap = max_gb.map_or(config.max_bytes, |gb| (gb * (1u64 << 30) as f64) as u64);
            let removed = prune_to_cap(&config.root, cap, None)?;
            println!("removed {} directories", removed.len());
        }
    }
    Ok(())
}

fn dispatch(
    cli: Cli,
    cache_config: Option<coderipper::build_cache::CacheConfig>,
) -> anyhow::Result<()> {
    match cli.command {
        Command::Fast {
            project,
            workspace,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Fast, None, workspace),
        Command::Sweep {
            project,
            workspace,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Sweep, None, workspace),
        Command::Check {
            id,
            project,
            workspace,
            portfolio_root,
        } => run_and_report(project, portfolio_root, Tier::Fast, Some(id), workspace),
        Command::Cache { action } => cache_command(action, cache_config),
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
    workspace: bool,
) -> anyhow::Result<()> {
    let project_root = project.unwrap_or(std::env::current_dir()?).canonicalize()?;
    let portfolio_root = portfolio_root
        .or_else(|| project_root.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project_root.clone());

    let ctx = CheckContext {
        project_root,
        portfolio_root,
    };

    let result = if workspace {
        let run = coderipper::run_workspace(
            &ctx,
            tier,
            only_check_id.as_deref(),
            &mut |name, index, total| eprintln!("coderipper: member {name} ({index}/{total})"),
        );
        eprintln!(
            "coderipper: workspace: {} members analysed, {} with errors",
            run.members, run.members_with_errors
        );
        run.result
    } else {
        coderipper::run_checks(&ctx, tier, only_check_id.as_deref())
    };

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
            f.severity,
            f.confidence,
            // a `--workspace` finding leads with its package (two members can share a directory name)
            f.member.as_deref().unwrap_or(&f.project),
            f.summary,
            f.check_id
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
