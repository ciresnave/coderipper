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

/// `--profile`'s values: which rules run and what is reported.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Profile {
    /// The five original checks and exactly the output they have always had (the default).
    Classic,
    /// `classic`, plus the coverage report; the rules of the language-neutral and language modules join it as they arrive.
    Extended,
}

/// `--coverage`'s values: how much of the gap list to print.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CoverageMode {
    /// Counts only (the default for `--coverage` alone).
    Summary,
    /// Counts and the ids of every rule not yet covered.
    Full,
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
    version,
    about = "Audits a Rust project for what clippy does not report — run it next to cargo clippy."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// What the three run subcommands (`fast`, `sweep`, `check`) share.
#[derive(clap::Args)]
struct RunOpts {
    /// Project to check. Defaults to the current directory.
    #[arg(long)]
    project: Option<PathBuf>,
    /// Check every member of the cargo workspace containing the project, not just the project.
    #[arg(long)]
    workspace: bool,
    /// Root of the portfolio, for checks that read sibling projects. Defaults to the parent of `project`.
    #[arg(long)]
    portfolio_root: Option<PathBuf>,
    /// Exit 1 when a finding is at least this severe. Without it findings are printed but never fail the run, like
    /// clippy's warnings; CI usually wants `--deny medium`.
    #[arg(long, value_enum)]
    deny: Option<Deny>,
    /// How to write findings to stdout: `human` (default) or `json` (one object per line, like cargo's own
    /// `--message-format json`).
    #[arg(long, value_enum, default_value = "human")]
    message_format: MessageFormat,
    /// Which rules run and what is reported: `classic` (default: the five original checks, output unchanged) or `extended` (adds
    /// the coverage report). A rule CodeRipper has not built yet is "not yet implemented": it is reported, never a finding, and
    /// it never fails a run.
    #[arg(long, value_enum, default_value = "classic")]
    profile: Profile,
    /// Print the coverage report even under the `classic` profile. `--coverage=full` also lists every rule not yet covered.
    #[arg(
        long,
        value_enum,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "summary"
    )]
    coverage: Option<CoverageMode>,
}

#[derive(Subcommand)]
enum Command {
    /// Run every fast-tier check (local-only: no registry or GitHub API calls). Meant for CI and
    /// on-demand use next to `cargo clippy`.
    Fast {
        #[command(flatten)]
        opts: RunOpts,
    },
    /// Run every check, including network-required ones (registries, GitHub API). Meant for a
    /// periodic or PM-triggered pass, not a per-PR gate.
    Sweep {
        #[command(flatten)]
        opts: RunOpts,
    },
    /// Run exactly one check by id, at any tier.
    Check {
        /// The id of the check (see the list in the README), e.g. `reachability`.
        id: String,
        #[command(flatten)]
        opts: RunOpts,
    },
    /// Run a module's conformance fixtures: which of its claims to cover a rule are earned by a seeded defect and a clean
    /// twin. Exits 1 when a fixture contradicts a claim, 3 when a rule could not run. Meant for a module's own CI.
    /// Fixtures are code that runs (the checks build them): use only fixtures you trust.
    Conformance {
        /// The module to judge. Only the built-in `rust` module exists so far.
        #[arg(long, default_value = "rust")]
        module: String,
        /// Judge only this rule.
        #[arg(long)]
        rule: Option<String>,
        /// The directory of fixtures: `<rule>/defective` and `<rule>/clean`, each with an `expect.toml`. Relative to the current
        /// directory, so run it from a source checkout (an installed binary does not carry the fixtures).
        #[arg(long, default_value = "conformance")]
        fixtures: PathBuf,
        /// Allow fixtures that need the network to run.
        #[arg(long)]
        network: bool,
        /// Exit 1 unless every judged rule is proven (a claim with no fixture, or one that needs the network without
        /// `--network`, is then a failure instead of a note).
        #[arg(long)]
        require_proven: bool,
        /// How to write the verdicts to stdout: `human` (default) or `json` (one `coderipper-conformance` object per rule).
        #[arg(long, value_enum, default_value = "human")]
        message_format: MessageFormat,
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
    run_named("coderipper", args)
}

/// [`run`], with `bin_name` as the name the usage line shows (`cargo coderipper` for the cargo subcommand).
fn run_named(bin_name: &str, args: Vec<OsString>) -> ExitCode {
    let matches = <Cli as clap::CommandFactory>::command()
        .bin_name(bin_name)
        .get_matches_from(args);
    let cli = match <Cli as clap::FromArgMatches>::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
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

fn conformance_command(
    module: &str,
    rule: Option<String>,
    fixtures: PathBuf,
    network: bool,
    require_proven: bool,
    message_format: MessageFormat,
) -> anyhow::Result<u8> {
    use crate::conformance::{run, Options, Verdict};
    if module != "rust" {
        return Err(UsageError(format!(
            "no module named \"{module}\"; the only module so far is the built-in \"rust\""
        ))
        .into());
    }
    let checks = crate::registered_checks();
    if let Some(wanted) = &rule {
        if !checks.iter().any(|c| c.id() == wanted) {
            let known: Vec<&str> = checks.iter().map(|c| c.id()).collect();
            return Err(UsageError(format!(
                "the module claims no rule \"{wanted}\"; it claims: {}",
                known.join(", ")
            ))
            .into());
        }
    }
    if !fixtures.is_dir() {
        return Err(UsageError(format!(
            "the fixtures directory {} does not exist; run this from a source checkout of the module, or pass --fixtures",
            fixtures.display()
        ))
        .into());
    }
    let module = crate::module::RustModule::new(&checks);
    let mut options = Options::new(fixtures).network(network);
    if let Some(rule) = rule {
        options = options.rule(rule);
    }
    let result = run(&module, &options)?;
    let mut failed = result.any_failed();
    let errored = result.any_errored();
    for proof in &result.rules {
        let (word, notes): (&str, Vec<String>) = match &proof.verdict {
            Verdict::Proven => ("proven", Vec::new()),
            Verdict::Failed(reasons) => ("FAILED", reasons.clone()),
            Verdict::Errored(problems) => ("ERROR", problems.clone()),
            Verdict::Unproven(why) => ("unproven", vec![why.clone()]),
            Verdict::NoFixture => ("no fixture", Vec::new()),
        };
        if require_proven && proof.verdict != Verdict::Proven {
            failed = true;
        }
        match message_format {
            MessageFormat::Human => {
                println!("{}: {word}", proof.rule);
                for note in notes {
                    println!("  {note}");
                }
            }
            MessageFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "reason": "coderipper-conformance",
                    "rule": proof.rule,
                    "verdict": match &proof.verdict {
                        Verdict::Proven => "proven",
                        Verdict::Failed(_) => "failed",
                        Verdict::Errored(_) => "error",
                        Verdict::Unproven(_) => "unproven",
                        Verdict::NoFixture => "no-fixture",
                    },
                    "notes": notes,
                })
            ),
        }
    }
    // A rule that could not run outranks a contradiction, as an error outranks a finding in a normal run.
    Ok(if errored {
        EXIT_COULD_NOT_RUN
    } else if failed {
        EXIT_FINDINGS
    } else {
        EXIT_CLEAN
    })
}

/// A run: what `fast`, `sweep` and `check` each add to the shared options.
struct RunArgs {
    opts: RunOpts,
    tier: Tier,
    only_check_id: Option<String>,
}

fn dispatch(cli: Cli, cache_config: Option<crate::build_cache::CacheConfig>) -> anyhow::Result<u8> {
    match cli.command {
        Command::Fast { opts } => run_and_report(RunArgs {
            opts,
            tier: Tier::Fast,
            only_check_id: None,
        }),
        Command::Sweep { opts } => run_and_report(RunArgs {
            opts,
            tier: Tier::Sweep,
            only_check_id: None,
        }),
        Command::Check { id, opts } => run_and_report(RunArgs {
            opts,
            tier: Tier::Fast,
            only_check_id: Some(id),
        }),
        Command::Conformance {
            module,
            rule,
            fixtures,
            network,
            require_proven,
            message_format,
        } => conformance_command(
            &module,
            rule,
            fixtures,
            network,
            require_proven,
            message_format,
        ),
        Command::Cache { action } => cache_command(action, cache_config),
        Command::Serve { port } => {
            anyhow::bail!("server mode is not implemented yet (port {port} requested)")
        }
    }
}

fn run_and_report(args: RunArgs) -> anyhow::Result<u8> {
    let RunArgs {
        opts:
            RunOpts {
                project,
                workspace,
                portfolio_root,
                deny,
                message_format,
                profile,
                coverage,
            },
        tier,
        only_check_id,
    } = args;
    if let Some(id) = &only_check_id {
        let known: Vec<&str> = crate::registered_checks().iter().map(|c| c.id()).collect();
        if !known.contains(&id.as_str()) {
            return Err(UsageError(format!(
                "no check named \"{id}\"; the checks are: {}",
                known.join(", ")
            ))
            .into());
        }
    }
    let project = project.unwrap_or(std::env::current_dir()?);
    let project_root = project
        .canonicalize()
        .map(without_verbatim_prefix)
        .map_err(|e| {
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

    // The coverage report: computed from the catalog joined with what the Rust module claims and has proven, and with this run's
    // outcomes. A gap in it never touches `code` above.
    let unchecked = crate::coverage::unchecked_languages(&ctx.project_root);
    let show_coverage = profile == Profile::Extended || coverage.is_some();
    let reports = show_coverage.then(|| coverage_reports(&result, &unchecked));

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
            if let Some(reports) = &reports {
                print_coverage_human(reports, coverage == Some(CoverageMode::Full));
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
            for report in reports.iter().flatten() {
                let limit = if coverage == Some(CoverageMode::Full) {
                    usize::MAX
                } else {
                    crate::coverage::GAP_LIST_LIMIT
                };
                println!("{}", report.to_json_with_gap_limit(limit));
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
    // Under `classic` nothing else says that part of the repository was not looked at, so one stderr line does (design 10.2).
    if reports.is_none() {
        for u in &unchecked {
            let s = if u.source_files == 1 { "" } else { "s" };
            eprintln!(
                "coderipper: {}: {} source file{s} found, not checked (classic profile; see --profile extended)",
                u.language, u.source_files
            );
        }
    }
    if !result.errors.is_empty() {
        eprintln!(
            "coderipper: {} error(s) during the run (see above)",
            result.errors.len()
        );
    }
    Ok(code)
}

/// The coverage of the Rust module for this run, then one entry per language found that no module checks.
fn coverage_reports(
    result: &crate::RunResult,
    unchecked: &[crate::coverage::UncheckedLanguage],
) -> Vec<crate::coverage::LanguageCoverage> {
    use crate::coverage::{for_missing_module, for_module};
    use crate::module::Module;
    let catalog = crate::catalog::Catalog::builtin();
    let checks = crate::registered_checks();
    let mut reports = Vec::new();
    if let Ok(hello) = crate::module::RustModule::new(&checks).describe() {
        reports.push(for_module(catalog, &hello, "rust", Some(&result.outcomes)));
    }
    for u in unchecked {
        reports.push(for_missing_module(catalog, &u.language, u.source_files));
    }
    reports
}

/// The coverage lines, then (when anything is not covered) the "not yet implemented" section. The wording is deliberate: a gap is
/// CodeRipper's own incomplete coverage, not a finding about the code that was checked, and it never fails a run.
fn print_coverage_human(reports: &[crate::coverage::LanguageCoverage], full: bool) {
    for report in reports {
        println!("{}", report.human_line());
    }
    if reports.iter().any(|r| !r.gaps.is_empty()) {
        println!(
            "Not yet implemented: CodeRipper has not built these checks yet. This says nothing about your code and never fails a run."
        );
        if full {
            for report in reports.iter().filter(|r| !r.gaps.is_empty()) {
                println!("  {}: {}", report.language, report.gaps.join(", "));
            }
        } else {
            println!("  (use --coverage=full to list them)");
        }
    }
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
                    | "conformance"
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
    run_named("cargo coderipper", args)
}

/// `canonicalize` on Windows returns the verbatim form `\\?\C:\dir`, which is noise in a message and in the JSON a CI job
/// parses. Strips the prefix from a drive path (a `\\?\UNC\server\share` path keeps it: it has no shorter spelling).
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(strip_verbatim_drive) {
        Some(plain) => PathBuf::from(plain),
        None => path,
    }
}

fn strip_verbatim_drive(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(r"\\?\")?;
    let mut chars = rest.chars();
    let drive = chars.next()?;
    (drive.is_ascii_alphabetic() && chars.next() == Some(':')).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::strip_verbatim_drive;

    #[test]
    fn a_verbatim_drive_path_loses_its_prefix() {
        assert_eq!(
            strip_verbatim_drive(r"\\?\C:\Users\me\proj"),
            Some(r"C:\Users\me\proj")
        );
    }

    #[test]
    fn other_paths_are_left_alone() {
        assert_eq!(strip_verbatim_drive(r"C:\Users\me"), None);
        assert_eq!(strip_verbatim_drive("/home/me/proj"), None);
        assert_eq!(
            strip_verbatim_drive(r"\\?\UNC\server\share\x"),
            None,
            "a UNC path has no shorter spelling"
        );
        assert_eq!(strip_verbatim_drive(r"\\?\Volume{1234}\x"), None);
    }
}
