//! The command-line program, shared by the `coderipper` and `cargo-coderipper` binaries. Not part of the library API.

use crate::check::{CheckContext, Tier};
use crate::finding::Severity;
use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// Exit codes (a promise to CI, documented in the README):
const EXIT_CLEAN: u8 = 0; // every check ran and nothing is at or above `--deny` (no `--deny`: any run that finished)
const EXIT_FINDINGS: u8 = 1; // a finding at or above `--deny`
const EXIT_USAGE: u8 = 2; // bad arguments (clap exits 2 itself) or a project path that cannot be read
const EXIT_COULD_NOT_RUN: u8 = 3; // a check could not run, so part of the audit did not happen

/// The invocation was wrong (exit 2), as opposed to the audit failing (exit 3).
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// How findings are written to stdout.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum MessageFormat {
    /// Human-readable lines (the default).
    Human,
    /// One JSON object per line, each with a `"reason"`: `coderipper-finding` per finding, then one
    /// `coderipper-summary` with the finding count, the errors and the exit code.
    Json,
}

/// `--deny`'s values: the severity at which a finding fails the run.
#[derive(Clone, Copy, ValueEnum)]
enum Deny {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Deny {
    fn severity(self) -> Severity {
        match self {
            Deny::Info => Severity::Info,
            Deny::Low => Severity::Low,
            Deny::Medium => Severity::Medium,
            Deny::High => Severity::High,
            Deny::Critical => Severity::Critical,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "coderipper",
    about = "Audits a Rust project for what clippy does not report — run it next to cargo clippy."
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
        /// Exit 1 when a finding is at least this severe (info, low, medium, high, critical). Without it findings are
        /// printed but never fail the run, like clippy's warnings; CI usually wants `--deny medium`.
        #[arg(long, value_enum)]
        deny: Option<Deny>,
        /// How to write findings to stdout: `human` (default) or `json` (one object per line, like cargo's own
        /// `--message-format json`).
        #[arg(long, value_enum, default_value = "human")]
        message_format: MessageFormat,
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
        /// Exit 1 when a finding is at least this severe (info, low, medium, high, critical). Without it findings are
        /// printed but never fail the run, like clippy's warnings; CI usually wants `--deny medium`.
        #[arg(long, value_enum)]
        deny: Option<Deny>,
        /// How to write findings to stdout: `human` (default) or `json` (one object per line, like cargo's own
        /// `--message-format json`).
        #[arg(long, value_enum, default_value = "human")]
        message_format: MessageFormat,
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
        /// Exit 1 when a finding is at least this severe (info, low, medium, high, critical). Without it findings are
        /// printed but never fail the run, like clippy's warnings; CI usually wants `--deny medium`.
        #[arg(long, value_enum)]
        deny: Option<Deny>,
        /// How to write findings to stdout: `human` (default) or `json` (one object per line, like cargo's own
        /// `--message-format json`).
        #[arg(long, value_enum, default_value = "human")]
        message_format: MessageFormat,
        portfolio_root: Option<PathBuf>,
    },
    /// Inspect or trim the build cache (the persistent target directory the checks build in).
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Run as an HTTP server (not implemented; planned for a hosted instance). Hidden from `--help`.
    #[command(hide = true)]
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

/// Runs the CLI over `args` (the first is the program name, as in `std::env::args_os`) and returns its exit code.
pub fn run(args: Vec<OsString>) -> ExitCode {
    let cli = Cli::parse_from(args);
    let from_env = crate::build_cache::config_from_env(&|k| std::env::var(k).ok());
    if let Some(config) = from_env.clone() {
        crate::build_cache::set_cache_config(config);
    }

    let result = dispatch(cli, from_env);
    // What the builds did, once, whichever way the run ended.
    if let Some(text) = crate::build_cache::render_stats(&crate::build_cache::take_stats()) {
        eprintln!("{text}");
    }
    ExitCode::from(match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("coderipper: {e:#}");
            if e.is::<UsageError>() {
                EXIT_USAGE
            } else {
                EXIT_COULD_NOT_RUN
            }
        }
    })
}

fn cache_command(
    action: CacheAction,
    config: Option<crate::build_cache::CacheConfig>,
) -> anyhow::Result<u8> {
    use crate::build_cache::{prune_to_cap, status};
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
    Ok(EXIT_CLEAN)
}

/// What the three run subcommands (`fast`, `sweep`, `check`) share.
struct RunArgs {
    project: Option<PathBuf>,
    portfolio_root: Option<PathBuf>,
    tier: Tier,
    only_check_id: Option<String>,
    workspace: bool,
    deny: Option<Deny>,
    message_format: MessageFormat,
}

fn dispatch(cli: Cli, cache_config: Option<crate::build_cache::CacheConfig>) -> anyhow::Result<u8> {
    match cli.command {
        Command::Fast {
            project,
            workspace,
            deny,
            message_format,
            portfolio_root,
        } => run_and_report(RunArgs {
            project,
            portfolio_root,
            tier: Tier::Fast,
            only_check_id: None,
            workspace,
            deny,
            message_format,
        }),
        Command::Sweep {
            project,
            workspace,
            deny,
            message_format,
            portfolio_root,
        } => run_and_report(RunArgs {
            project,
            portfolio_root,
            tier: Tier::Sweep,
            only_check_id: None,
            workspace,
            deny,
            message_format,
        }),
        Command::Check {
            id,
            project,
            workspace,
            deny,
            message_format,
            portfolio_root,
        } => run_and_report(RunArgs {
            project,
            portfolio_root,
            tier: Tier::Fast,
            only_check_id: Some(id),
            workspace,
            deny,
            message_format,
        }),
        Command::Cache { action } => cache_command(action, cache_config),
        Command::Serve { port } => {
            anyhow::bail!(
                "server mode is not implemented yet (port {port} requested) — see design doc §7"
            )
        }
    }
}

fn run_and_report(args: RunArgs) -> anyhow::Result<u8> {
    let RunArgs {
        project,
        portfolio_root,
        tier,
        only_check_id,
        workspace,
        deny,
        message_format,
    } = args;
    let project = project.unwrap_or(std::env::current_dir()?);
    let project_root = project.canonicalize().map_err(|e| {
        UsageError(format!(
            "cannot read the project {}: {e}",
            project.display()
        ))
    })?;
    let portfolio_root = portfolio_root
        .or_else(|| project_root.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project_root.clone());

    let ctx = CheckContext::new(project_root).portfolio_root(portfolio_root);

    let result = if workspace {
        let run = crate::run_workspace(
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
        crate::run_checks(&ctx, tier, only_check_id.as_deref())
    };

    // A check error outranks a finding: a run that could not judge everything must not look like a complete one
    // that found something (review finding: errors used to be printed but the process still exited 0).
    let denied = deny.map(Deny::severity);
    let code = if !result.errors.is_empty() {
        EXIT_COULD_NOT_RUN
    } else if denied.is_some_and(|level| result.findings.iter().any(|f| f.severity >= level)) {
        EXIT_FINDINGS
    } else {
        EXIT_CLEAN
    };

    match message_format {
        MessageFormat::Human => {
            // Review finding: this used to print "no checks registered yet" whenever BOTH findings and
            // errors were empty -- which is what a genuinely clean run also looks like. Distinguish the real cases.
            if result.findings.is_empty() && result.errors.is_empty() {
                println!("coderipper: no issues found");
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
        }
        MessageFormat::Json => {
            for f in &result.findings {
                let mut line = serde_json::to_value(f)?;
                let object = line
                    .as_object_mut()
                    .expect("a Finding serializes to an object");
                object.insert("reason".into(), "coderipper-finding".into());
                println!("{line}");
            }
            println!(
                "{}",
                serde_json::json!({
                    "reason": "coderipper-summary",
                    "findings": result.findings.len(),
                    "errors": result.errors,
                    "exit_code": code,
                })
            );
        }
    }
    for e in &result.errors {
        eprintln!("coderipper: check error: {e}");
    }
    if !result.errors.is_empty() {
        eprintln!(
            "coderipper: {} error(s) during the run (see above)",
            result.errors.len()
        );
    }
    Ok(code)
}

/// Runs the CLI as the cargo subcommand `cargo coderipper`. Cargo runs `cargo-coderipper coderipper <args>` (it passes
/// the subcommand's own name first), and a bare `cargo coderipper` (nothing, or only options) means `fast`.
pub fn run_as_cargo_subcommand(mut args: Vec<OsString>) -> ExitCode {
    if args.get(1).is_some_and(|a| a == "coderipper") {
        args.remove(1);
    }
    let names_a_subcommand = args.get(1).is_some_and(|a| {
        matches!(
            a.to_str(),
            Some(
                "fast"
                    | "sweep"
                    | "check"
                    | "cache"
                    | "serve"
                    | "help"
                    | "-h"
                    | "--help"
                    | "-V"
                    | "--version"
            )
        )
    });
    if !names_a_subcommand {
        args.insert(1, "fast".into());
    }
    run(args)
}
