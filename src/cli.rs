// SPDX-License-Identifier: MIT OR Apache-2.0
//! The command-line program, shared by the `coderipper` and `cargo-coderipper` binaries. Not part of the library API.

use crate::check::{CheckContext, Tier};
use crate::config::{Deny, MessageFormat, Profile};
use crate::finding::Severity;
use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// Exit codes (a promise to CI, documented in the README):
const EXIT_CLEAN: u8 = 0; // every check ran and no finding is at or above `--deny` (default `info`: no finding at all; `none`: any run that finished)
const EXIT_FINDINGS: u8 = 1; // a finding at or above `--deny`, and no `[[allow]]` entry waives it
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

impl Profile {
    fn lib(self) -> crate::Profile {
        match self {
            Profile::Classic => crate::Profile::Classic,
            Profile::Extended => crate::Profile::Extended,
            Profile::Portfolio => crate::Profile::Portfolio,
        }
    }
}

/// `--coverage`'s values: how much of the gap list to print.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CoverageMode {
    /// Counts only (the default for `--coverage` alone).
    Summary,
    /// Counts and the ids of every rule not yet covered.
    Full,
}

impl Deny {
    /// The lowest severity that fails the run; `None` when nothing does.
    fn severity(self) -> Option<Severity> {
        match self {
            Deny::None => None,
            Deny::Info => Some(Severity::Info),
            Deny::Low => Some(Severity::Low),
            Deny::Medium => Some(Severity::Medium),
            Deny::High => Some(Severity::High),
            Deny::Critical => Some(Severity::Critical),
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
    /// Read exactly this configuration file in place of the system and user files (`/etc/coderipper/.coderipper.toml`,
    /// `/etc/.coderipper.toml`, `~/.coderipper.toml`). It must exist. The repository's own file is read as always.
    /// The variable `CODERIPPER_CONFIG` does the same, and `--no-config` outranks it.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
    /// Read none of the system and user configuration files, so the result does not depend on this machine. The repository's
    /// own `.coderipper.toml` is still read.
    /// The variable `CODERIPPER_NO_CONFIG` (`1`, `true`, `yes` or `on`) does the same.
    #[arg(long, global = true)]
    no_config: bool,
    #[command(subcommand)]
    command: Command,
}

/// What the configuration layers gave this process, and which of the three options that have a built-in default were written on
/// the command line (a flag the user typed outranks every file; a default that was merely there does not).
struct Conf {
    settings: crate::config::Settings,
    given: Given,
}

#[derive(Clone, Copy, Default)]
struct Given {
    deny: bool,
    profile: bool,
    message_format: bool,
}

impl Conf {
    /// The environment variable `key`, else what a configuration file gave for it: the environment always wins.
    fn lookup(&self, key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| self.settings.env_default(key))
    }

    fn deny(&self, flag: Deny) -> Deny {
        match (self.settings.deny, self.given.deny) {
            (Some(file), false) => file,
            _ => flag,
        }
    }

    fn message_format(&self, flag: MessageFormat) -> MessageFormat {
        match (self.settings.message_format, self.given.message_format) {
            (Some(file), false) => file,
            _ => flag,
        }
    }

    /// `--profile`, else the repository's `profile`, else a wide file's, else the built-in default.
    fn profile(&self, flag: Profile, repository: Option<Profile>) -> Profile {
        if self.given.profile {
            return flag;
        }
        repository.or(self.settings.profile).unwrap_or(flag)
    }

    fn redact(&self, flag: bool) -> bool {
        flag || self.settings.redact == Some(true)
    }

    fn install_tools(&self, flag: bool) -> bool {
        flag || self.settings.install_tools == Some(true)
    }
}

/// What the three run subcommands (`fast`, `sweep`, `check`) share.
#[derive(clap::Args)]
struct RunOpts {
    /// Project to check. Defaults to the current directory. `reachability`, `unused-return-values` and `unused-parameters`
    /// analyse the committed tree (`HEAD`) in a throwaway worktree: uncommitted changes (tracked or untracked) are not analysed by
    /// them, and the run says so when there are any; `version-consistency` and API-006 read the working tree.
    #[arg(long)]
    project: Option<PathBuf>,
    /// Check every member of the cargo workspace containing the project, not just the project.
    #[arg(long)]
    workspace: bool,
    /// Root of the portfolio, for checks that read sibling projects. Defaults to the parent of `project`.
    #[arg(long)]
    portfolio_root: Option<PathBuf>,
    /// Cargo features to enable when a check builds the crate, comma-separated (repeatable), as `cargo build --features`.
    /// A target whose `required-features` are not enabled is not built and so is not checked; the run says which.
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,
    /// Enable every cargo feature when a check builds the crate, as `cargo build --all-features`.
    #[arg(long)]
    all_features: bool,
    /// Do not enable the default cargo features when a check builds the crate, as `cargo build --no-default-features`.
    #[arg(long)]
    no_default_features: bool,
    /// Exit 1 when a finding is at least this severe and no `[[allow]]` entry waives it. The default is `info`: any finding
    /// fails the run. `--deny medium` lets Low and Info findings through; `--deny none` never fails on a finding.
    #[arg(long, value_enum, default_value = "info")]
    deny: Deny,
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
    /// Say that the tools the delegated rules need (`--profile extended`: gitleaks for SEC-002, osv-scanner for SUP-002 and SEC-006, lychee for DOC-010, zizmor for SUP-008, buf for API-006) may be downloaded into the tool
    /// cache when they are not there. Without it nothing is downloaded or written, in CI or anywhere else, and a rule whose tool is
    /// missing is reported as not run (a gap; it never fails the run).
    #[arg(long)]
    install_tools: bool,
    /// A tools lock to use instead of the one built into this binary.
    #[arg(long)]
    tools_lock: Option<PathBuf>,
    /// Remove known secret shapes (URL passwords, `KEY=value` for token/secret/password keys, `Authorization` headers, well-known
    /// token prefixes, private-key blocks) from what builds and tools wrote and this run prints, and withhold a failed build
    /// script's own output and the stderr of a tool or module that crashed. Off by default: on your own machine that output is
    /// yours. It removes known shapes only; it does not promise that no secret remains, and a compiler message that quotes your
    /// source is shown with its known shapes removed, not withheld.
    #[arg(long)]
    redact: bool,
    /// Do not run a built-in check, and say why: `--skip-rule ID=WHY` (repeatable; `ID` is a check such as
    /// `ci-protection-presence`, not a rule of `--profile extended`). The run prints the reason as a note. For a check that cannot
    /// run where the run is (it needs repository-admin read, say), with the place it does run named in `WHY`; not a way to
    /// silence a finding, which is an `[[allow]]` entry.
    #[arg(long = "skip-rule", value_name = "ID=WHY")]
    skip_rule: Vec<String>,
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
        /// The module to judge: the built-in `rust` module (the default), `neutral` (the language-neutral rules) or `delegated`
        /// (the rules run by installed tools).
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
        /// Say that the tool a delegated rule runs may be downloaded when it is not installed (without it that rule is unproven).
        #[arg(long)]
        install_tools: bool,
        /// A tools lock to use instead of the one built into this binary.
        #[arg(long)]
        tools_lock: Option<PathBuf>,
        /// Exit 1 unless every judged rule is proven (a claim with no fixture, or one that needs the network without
        /// `--network`, is then a failure instead of a note).
        #[arg(long)]
        require_proven: bool,
        /// How to write the verdicts to stdout: `human` (default) or `json` (one `coderipper-conformance` object per rule).
        #[arg(long, value_enum, default_value = "human")]
        message_format: MessageFormat,
        /// Remove known secret shapes from what builds and tools wrote and the reasons print, as `--redact` does for `check`.
        /// Off by default; it removes known shapes only and changes no verdict and no exit code.
        #[arg(long)]
        redact: bool,
    },
    /// Inspect or trim the build cache (the persistent target directory the checks build in).
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// The external tools CodeRipper may install for delegated rules: what is pinned, and installing it (only with your say-so).
    Tools {
        #[command(subcommand)]
        action: ToolsAction,
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

#[derive(Subcommand)]
enum ToolsAction {
    /// The ledger: every pinned tool with its version, licence, source and whether it is installed. Installs nothing.
    List {
        /// A tools lock to use instead of the one built into this binary.
        #[arg(long)]
        tools_lock: Option<PathBuf>,
    },
    /// Install pinned tools into the isolated tool cache. Needs `--install-tools`: CodeRipper installs nothing without it.
    Install {
        /// The tools to install; none means every tool the lock has for this platform.
        names: Vec<String>,
        /// Say that installing is allowed. Without it nothing is downloaded or written, in CI or anywhere else.
        #[arg(long)]
        install_tools: bool,
        /// A tools lock to use instead of the one built into this binary.
        #[arg(long)]
        tools_lock: Option<PathBuf>,
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
    let on_command_line = |m: &clap::ArgMatches, id: &str| {
        m.try_contains_id(id).unwrap_or(false)
            && m.value_source(id) == Some(clap::parser::ValueSource::CommandLine)
    };
    if on_command_line(&matches, "config") && on_command_line(&matches, "no_config") {
        eprintln!("coderipper: --config and --no-config cannot be used together");
        return ExitCode::from(EXIT_USAGE);
    }
    let sub = matches.subcommand().map(|(_, m)| m);
    let given = Given {
        deny: sub.is_some_and(|m| on_command_line(m, "deny")),
        profile: sub.is_some_and(|m| on_command_line(m, "profile")),
        message_format: sub.is_some_and(|m| on_command_line(m, "message_format")),
    };
    // The wide files, read once. A file that cannot be read or parsed stops the run: a result must never come from settings the
    // reader cannot see.
    let environment = |k: &str| std::env::var(k).ok();
    let no_config = cli.no_config
        || environment("CODERIPPER_NO_CONFIG").is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
    let config_path = cli.config.clone().or_else(|| {
        environment("CODERIPPER_CONFIG")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
    });
    let layers = match crate::config::load_wide(&environment, config_path.as_deref(), no_config) {
        Ok(layers) => layers,
        Err(e) => {
            eprintln!("coderipper: {e:#}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let mut settings = crate::config::Settings::from_layers(&layers);
    settings.apply_env_limits(&environment);
    report_wide_configuration(&layers, &settings);
    crate::config::install(layers);
    let conf = Conf { settings, given };
    let from_env = crate::build_cache::config_from_env(&|k| conf.lookup(k));
    if let Some(config) = from_env.clone() {
        // `false`: a library caller in this process configured the cache first, and that configuration stays.
        if !crate::build_cache::set_cache_config(config) {
            eprintln!("coderipper: the build cache was already configured in this process; keeping that configuration");
        }
    }

    let result = dispatch(cli, from_env, &conf);
    // What the builds did, once, whichever way the run ended.
    if let Some(text) = crate::build_cache::render_stats(&crate::build_cache::take_stats()) {
        eprintln!("{}", crate::redact::scrub(&text));
    }
    ExitCode::from(match result {
        Ok(code) => code,
        Err(e) => {
            // A fixture's own text can reach an error (a `toml` parse error quotes the line); a no-op unless `--redact`.
            eprintln!("coderipper: {}", crate::redact::scrub(&format!("{e:#}")));
            if e.is::<UsageError>() {
                EXIT_USAGE
            } else {
                EXIT_COULD_NOT_RUN
            }
        }
    })
}

/// Says, once and on stderr, which wide files were read, the ceilings in force and (one warning per file) the waivers a wide
/// file holds: those belong in each repository, where they are reviewed with the code, and they are applied anyway.
fn report_wide_configuration(layers: &[crate::config::Layer], settings: &crate::config::Settings) {
    if layers.is_empty() {
        return;
    }
    let files: Vec<String> = layers
        .iter()
        .map(|l| l.path.display().to_string())
        .collect();
    eprintln!("coderipper: configuration: {}", files.join(", "));
    for layer in layers {
        let n = layer.file.allow.len();
        if n > 0 {
            let noun = if n == 1 { "entry" } else { "entries" };
            eprintln!(
                "coderipper: warning: {} holds {n} [[allow]] {noun}; waivers belong in each repository's .coderipper.toml, where they are reviewed with the code. Applied anyway: a finding waived here is printed with this file's name.",
                layer.path.display()
            );
        }
        let p = layer
            .file
            .reachability
            .as_ref()
            .map_or(0, |r| r.public_api.len());
        if p > 0 {
            let noun = if p == 1 { "entry" } else { "entries" };
            eprintln!(
                "coderipper: warning: {} holds {p} [reachability] public_api {noun}; what a project declares public belongs in that project's .coderipper.toml, where it is reviewed with the code. Applied anyway: an item declared here is printed with this file's name.",
                layer.path.display()
            );
        }
    }
    for limit in &settings.limits {
        eprintln!(
            "coderipper: configuration: {} = {} (from {}; the process that runs CodeRipper enforces it)",
            limit.name, limit.value, limit.source
        );
    }
}

fn tools_command(action: ToolsAction, conf: &Conf) -> anyhow::Result<u8> {
    use crate::tools::{
        current_platform, tools_dir_from_env, Consent, DefaultFetcher, ToolCache, ToolError,
        ToolStatus, ToolsLock,
    };
    let (lock_path, names, install) = match action {
        ToolsAction::List { tools_lock } => (tools_lock, Vec::new(), None),
        ToolsAction::Install {
            names,
            install_tools,
            tools_lock,
        } => (tools_lock, names, Some(conf.install_tools(install_tools))),
    };
    let lock = match &lock_path {
        Some(path) => ToolsLock::parse(&std::fs::read_to_string(path).map_err(|e| {
            anyhow::anyhow!("tools_lock_invalid: cannot read {}: {e}", path.display())
        })?)?,
        None => ToolsLock::embedded(),
    };
    let root = tools_dir_from_env(&|k| conf.lookup(k)).ok_or_else(|| {
        anyhow::anyhow!("no tools directory could be derived; set CODERIPPER_TOOLS_DIR")
    })?;
    let cache = ToolCache::new(root);
    let platform = current_platform();
    let Some(install_tools) = install else {
        println!("tool cache: {}", cache.root().display());
        if lock.entries().is_empty() {
            println!("no tools are pinned yet (the delegated rules add theirs as they land)");
        }
        for entry in lock.entries() {
            let status = if entry.platform != platform {
                "other platform"
            } else {
                match cache.status(entry) {
                    ToolStatus::Installed => "installed",
                    ToolStatus::NotInstalled => "not installed",
                    ToolStatus::Corrupt => "corrupt (checksum differs from the lock)",
                }
            };
            println!(
                "{} {}  {}  {}  {}  {}",
                entry.name, entry.version, entry.platform, entry.licence, status, entry.source
            );
        }
        return Ok(EXIT_CLEAN);
    };
    let names: Vec<String> = if names.is_empty() {
        lock.entries()
            .iter()
            .filter(|e| e.platform == platform)
            .map(|e| e.name.clone())
            .collect()
    } else {
        names
    };
    if names.is_empty() {
        println!("nothing to install: the lock has no tools for {platform}");
    }
    // A name the lock does not have is a typo in the command line, not a tool that could not be had: check them all before
    // installing any, so a typo beside a valid name installs nothing.
    if let Some(unknown) = names.iter().find(|n| !lock.knows(n)) {
        return Err(
            UsageError(crate::tools::ToolError::Missing(unknown.clone()).to_string()).into(),
        );
    }
    let consent = if install_tools {
        Consent::Granted
    } else {
        Consent::NotGiven
    };
    for name in names {
        let path = cache
            .ensure(&lock, &name, &platform, consent, &DefaultFetcher)
            .map_err(|e| match (&lock_path, &e) {
                // The printed command must work: it has to name the same lock.
                (Some(lock), ToolError::ConsentNotGiven { .. }) => {
                    anyhow::anyhow!("{e} --tools-lock {}", lock.display())
                }
                _ => anyhow::Error::from(e),
            })?;
        let version = lock
            .for_platform(&name, &platform)
            .map_or("", |e| e.version.as_str());
        println!("{name} {version} at {}", path.display());
    }
    Ok(EXIT_CLEAN)
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

/// The tools the delegated rules may use: the lock (the built-in one, or `--tools-lock`), the environment's tools directory, and
/// consent to install only when `--install-tools` was passed.
fn tool_env(
    install_tools: bool,
    lock_path: Option<&Path>,
    conf: &Conf,
) -> anyhow::Result<crate::tools::ToolEnv> {
    use crate::tools::{Consent, ToolEnv, ToolsLock};
    let mut env = ToolEnv::from_environment().consent(if conf.install_tools(install_tools) {
        Consent::Granted
    } else {
        Consent::NotGiven
    });
    // the tools directory a wide file named, when the environment did not
    if let Some(dir) = crate::tools::tools_dir_from_env(&|k| conf.lookup(k)) {
        env = env.cache_root(dir);
    }
    if let Some(path) = lock_path {
        let text = std::fs::read_to_string(path).map_err(|e| {
            UsageError(format!(
                "tools_lock_invalid: cannot read {}: {e}",
                path.display()
            ))
        })?;
        env = env.lock(ToolsLock::parse(&text)?);
    }
    Ok(env)
}

fn conformance_command(
    module: &str,
    rule: Option<String>,
    fixtures: PathBuf,
    network: bool,
    tools: crate::tools::ToolEnv,
    require_proven: bool,
    message_format: MessageFormat,
) -> anyhow::Result<u8> {
    use crate::conformance::{run, Options, Verdict};
    if !matches!(module, "rust" | "neutral" | "delegated") {
        return Err(UsageError(format!(
            "no module named \"{module}\"; the built-in modules are \"rust\", \"neutral\" and \"delegated\""
        ))
        .into());
    }
    let checks = crate::registered_checks();
    let known: Vec<&str> = match module {
        "neutral" => crate::module::neutral_rule_ids(),
        "delegated" => crate::module::delegated_rule_ids(),
        _ => checks.iter().map(|c| c.id()).collect(),
    };
    if let Some(wanted) = &rule {
        if !known.contains(&wanted.as_str()) {
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
    let rust = crate::module::RustModule::new(&checks);
    let neutral = crate::module::NeutralModule::new();
    let delegated = crate::module::DelegatedModule::new(tools);
    let module: &dyn crate::module::Module = match module {
        "neutral" => &neutral,
        "delegated" => &delegated,
        _ => &rust,
    };
    let mut options = Options::new(fixtures).network(network);
    if let Some(rule) = rule {
        options = options.rule(rule);
    }
    let result = run(module, &options)?;
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
        // A reason can carry what a fixture's build or tool wrote; most are already cleaned where they were made, and this is
        // the last place a reason passes (a no-op unless `--redact`).
        let notes: Vec<String> = notes.iter().map(|n| crate::redact::scrub(n)).collect();
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

fn dispatch(
    cli: Cli,
    cache_config: Option<crate::build_cache::CacheConfig>,
    conf: &Conf,
) -> anyhow::Result<u8> {
    match cli.command {
        Command::Fast { opts } => run_and_report(
            RunArgs {
                opts,
                tier: Tier::Fast,
                only_check_id: None,
            },
            conf,
        ),
        Command::Sweep { opts } => run_and_report(
            RunArgs {
                opts,
                tier: Tier::Sweep,
                only_check_id: None,
            },
            conf,
        ),
        Command::Check { id, opts } => run_and_report(
            RunArgs {
                opts,
                tier: Tier::Fast,
                only_check_id: Some(id),
            },
            conf,
        ),
        Command::Conformance {
            module,
            rule,
            fixtures,
            network,
            install_tools,
            tools_lock,
            require_proven,
            message_format,
            redact,
        } => {
            if conf.redact(redact) {
                crate::redact::set_policy(crate::redact::RedactionPolicy::On);
            }
            conformance_command(
                &module,
                rule,
                fixtures,
                network,
                tool_env(install_tools, tools_lock.as_deref(), conf)?,
                require_proven,
                conf.message_format(message_format),
            )
        }
        Command::Cache { action } => cache_command(action, cache_config),
        Command::Tools { action } => tools_command(action, conf),
        Command::Serve { port } => {
            anyhow::bail!("server mode is not implemented yet (port {port} requested)")
        }
    }
}

/// What to tell the user when `project` has changes git has not committed (tracked edits or untracked files): `reachability`,
/// `unused-return-values` and `unused-parameters` read `HEAD` in a throwaway worktree, so a fix that is only in the working
/// tree changes nothing for them (`version-consistency` and API-006 do read the working tree). `None` when the tree is clean
/// or git cannot say.
fn uncommitted_note(project: &Path) -> Option<String> {
    let output = crate::github::git_command(
        project,
        &["status", "--porcelain", "--untracked-files=all", "--", "."],
    )
    .output()
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let changed = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    (changed > 0).then(|| {
        format!(
            "{changed} file(s) under {} have uncommitted changes (tracked edits or untracked files); reachability, unused-return-values and unused-parameters analyse the committed tree (HEAD) in a throwaway worktree, so those changes are not analysed by them: commit them to see their effect (version-consistency and API-006 read the working tree)",
            project.display()
        )
    })
}

fn run_and_report(args: RunArgs, conf: &Conf) -> anyhow::Result<u8> {
    let RunArgs {
        opts:
            RunOpts {
                project,
                workspace,
                portfolio_root,
                features,
                all_features,
                no_default_features,
                deny,
                message_format,
                profile,
                coverage,
                install_tools,
                tools_lock,
                redact,
                skip_rule,
            },
        tier,
        only_check_id,
    } = args;
    let skip = skip_rule
        .iter()
        .map(|s| match s.split_once('=') {
            Some((id, why)) if !id.trim().is_empty() && !why.trim().is_empty() => {
                Ok((id.trim().to_string(), why.trim().to_string()))
            }
            _ => Err(UsageError(format!(
                "--skip-rule takes ID=WHY, and a skip says why; got \"{s}\""
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
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
    // The repository's own file: its `profile` applies, and any other known key it sets is named and skipped. An error in it
    // (an unknown key) is reported by the run itself, as an allowlist error.
    let repository_file = project_root.join(crate::config::FILE_NAME);
    let repository = crate::config::read_file(&repository_file, false)
        .ok()
        .flatten();
    // a repository at the user's home (or at the file `--config` names): its file is a wide file, whose keys apply as such
    let is_a_wide_file = std::fs::canonicalize(&repository_file).is_ok_and(|own| {
        crate::config::wide_layers()
            .iter()
            .any(|l| std::fs::canonicalize(&l.path).ok().as_ref() == Some(&own))
    });
    if let Some(file) = repository.as_ref().filter(|_| !is_a_wide_file) {
        let skipped = file.skipped_for_a_repository();
        if !skipped.is_empty() {
            eprintln!(
                "coderipper: warning: {} sets {}, which a repository's file may not set (it may set {}); skipped",
                repository_file.display(),
                skipped
                    .iter()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                crate::config::REPOSITORY_MAY_SET
            );
        }
    }
    let deny = conf.deny(deny);
    let message_format = conf.message_format(message_format);
    let profile = conf.profile(profile, repository.as_ref().and_then(|f| f.profile));
    // `blocking = [..]` in the repository's file: rules it has made blocking under `--profile portfolio`; a name that is no rule is an error
    let blocking: Vec<String> = repository
        .as_ref()
        .map(|f| f.blocking.clone())
        .unwrap_or_default();
    if !blocking.is_empty() {
        let checks = crate::registered_checks();
        let mut known: Vec<&str> = checks.iter().map(|c| c.id()).collect();
        known.extend(crate::module::neutral_rule_ids());
        if let Some(bad) = blocking.iter().find(|id| !known.contains(&id.as_str())) {
            return Err(UsageError(format!(
                "{}: blocking names \"{bad}\", which is no rule of --profile portfolio; the rules are: {}",
                crate::config::FILE_NAME,
                known.join(", ")
            ))
            .into());
        }
    }
    if conf.redact(redact) {
        crate::redact::set_policy(crate::redact::RedactionPolicy::On);
    }
    let tools = tool_env(install_tools, tools_lock.as_deref(), conf)?;
    if let Some(id) = &only_check_id {
        let checks = crate::registered_checks();
        let mut known: Vec<&str> = checks.iter().map(|c| c.id()).collect();
        let mut neutral = crate::module::neutral_rule_ids();
        neutral.extend(crate::module::delegated_rule_ids());
        if profile == Profile::Portfolio {
            known.extend(crate::module::neutral_rule_ids());
        } else if profile == Profile::Extended {
            known.extend(&neutral);
        } else {
            // the one rule every profile takes
            known.push("conflict-marker");
        }
        if !known.contains(&id.as_str()) {
            let hint = if neutral.contains(&id.as_str()) {
                " (that rule belongs to --profile extended)"
            } else {
                ""
            };
            return Err(UsageError(format!(
                "no check named \"{id}\"{hint}; the checks are: {}",
                known.join(", ")
            ))
            .into());
        }
    }
    let portfolio_root = portfolio_root
        .or_else(|| project_root.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| project_root.clone());

    if let Some(note) = uncommitted_note(&project_root) {
        eprintln!("coderipper: note: {note}");
    }
    let mut cargo_features = crate::check::CargoFeatures::default().with_features(features);
    if all_features {
        cargo_features = cargo_features.all_features();
    }
    if no_default_features {
        cargo_features = cargo_features.no_default_features();
    }
    let ctx = CheckContext::new(project_root)
        .portfolio_root(portfolio_root)
        .cargo_features(cargo_features);

    let result = if workspace {
        let run = crate::run_workspace_in_with_tools_skipping(
            profile.lib(),
            &tools,
            &ctx,
            tier,
            only_check_id.as_deref(),
            &skip,
            &mut |name, index, total| eprintln!("coderipper: member {name} ({index}/{total})"),
        );
        eprintln!(
            "coderipper: workspace: {} members analysed, {} with errors",
            run.members, run.members_with_errors
        );
        run.result
    } else {
        crate::run_checks_in_with_tools_skipping(
            profile.lib(),
            &tools,
            &ctx,
            tier,
            only_check_id.as_deref(),
            &skip,
        )
    };

    // A check error outranks a finding: a run that could not judge everything must not look like a complete one
    // that found something (review finding: errors used to be printed but the process still exited 0).
    let denied = deny.severity();
    let code = if !result.errors.is_empty() {
        EXIT_COULD_NOT_RUN
    } else if profile == Profile::Portfolio {
        // a floor per rule instead of one `--deny`; `--deny none` still never fails on a finding
        let blocked = deny != Deny::None
            && result.findings.iter().any(|f| {
                let opted_in = blocking.contains(&f.check_id);
                crate::portfolio_floor(&f.check_id)
                    .or(opted_in.then_some(crate::finding::Severity::Low))
                    .is_some_and(|floor| f.severity >= floor)
            });
        if blocked {
            EXIT_FINDINGS
        } else {
            EXIT_CLEAN
        }
    } else if denied.is_some_and(|level| result.findings.iter().any(|f| f.severity >= level)) {
        EXIT_FINDINGS
    } else {
        EXIT_CLEAN
    };

    // The coverage report: computed from the catalog joined with what the Rust module claims and has proven, and with this run's
    // outcomes. A gap in it never touches `code` above.
    let unchecked = crate::coverage::unchecked_languages(&ctx.project_root);
    let show_coverage = profile == Profile::Extended || coverage.is_some();
    let reports = show_coverage
        .then(|| coverage_reports(&result, &unchecked, profile == Profile::Extended, &tools));

    match message_format {
        MessageFormat::Human => {
            // Review finding: this used to print "no checks registered yet" whenever BOTH findings and
            // errors were empty -- which is what a genuinely clean run also looks like. Distinguish the real cases.
            if result.findings.is_empty() && result.errors.is_empty() && !result.waived.is_empty() {
                println!(
                    "coderipper: no findings ({} waived, shown above)",
                    result.waived.len()
                );
            } else if result.findings.is_empty() && result.errors.is_empty() {
                if result.notes.is_empty() {
                    println!("coderipper: no issues found");
                } else {
                    println!(
                        "coderipper: no issues found by the rules that ran (see the {} note(s) on stderr)",
                        result.notes.len()
                    );
                }
            }
            for w in &result.waived {
                let f = &w.finding;
                println!(
                    "[{}/{:?}] {} — {}{} ({}) -- reason: {}",
                    if w.declared_public {
                        "declared-public"
                    } else {
                        "waived"
                    },
                    f.severity,
                    f.member.as_deref().unwrap_or(&f.project),
                    f.summary,
                    place(f),
                    f.check_id,
                    // one line, printable: the reason comes from a file the project edits
                    w.reason
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .collect::<String>()
                );
                // a waiver from a wide file is one nobody in the repository reviewed: say which file
                if let Some(source) = &w.source {
                    println!(
                        "  ({} by {source})",
                        if w.declared_public {
                            "declared"
                        } else {
                            "waived"
                        }
                    );
                }
            }
            for f in &result.findings {
                println!(
                    "[{:?}/{:?}] {} — {}{} ({})",
                    f.severity,
                    f.confidence,
                    // a `--workspace` finding leads with its package (two members can share a directory name)
                    f.member.as_deref().unwrap_or(&f.project),
                    f.summary,
                    place(f),
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
            for w in &result.waived {
                let mut line = serde_json::to_value(&w.finding)?;
                let object = line
                    .as_object_mut()
                    .expect("a Finding serializes to an object");
                object.insert("reason".into(), "coderipper-waived".into());
                object.insert("waiver_reason".into(), w.reason.clone().into());
                if let Some(source) = &w.source {
                    object.insert("waived_by".into(), source.clone().into());
                }
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
            let mut summary = serde_json::json!({
                "reason": "coderipper-summary",
                "findings": result.findings.len(),
                "waived": result.waived.len(),
                "errors": result.errors,
                "exit_code": code,
            });
            // Only when the run has a note (a rule not run for want of its tool, a count a rule set aside): the summary of every
            // other run keeps exactly its keys.
            if !result.notes.is_empty() {
                summary["notes"] = serde_json::json!(result.notes);
            }
            println!("{summary}");
        }
    }
    for e in &result.errors {
        eprintln!("coderipper: check error: {e}");
    }
    // A rule that did not run because its tool is not available: a gap, said once, with what to do. It never changes `code`.
    for note in &result.notes {
        eprintln!("coderipper: note: {note}");
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

/// ` at <file>:<line>` (or ` at <file>`) for a finding that has a location, printed before the check id so a line still ends in
/// `(<check id>)`. Several findings of one rule in one repository read the same without it (eight "actions/checkout@v4 is not
/// pinned" lines), and the JSON had the place all along. The file is third-party text: control characters are replaced.
fn place(f: &crate::finding::Finding) -> String {
    let Some(location) = &f.location else {
        return String::new();
    };
    let file: String = location
        .file
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    match location.line {
        Some(line) => format!(" at {file}:{line}"),
        None => format!(" at {file}"),
    }
}

/// The coverage of the Rust module for this run, then one entry per language found that no module checks.
fn coverage_reports(
    result: &crate::RunResult,
    unchecked: &[crate::coverage::UncheckedLanguage],
    with_neutral: bool,
    tools: &crate::tools::ToolEnv,
) -> Vec<crate::coverage::LanguageCoverage> {
    use crate::coverage::{for_missing_module_with, for_module};
    use crate::module::{Composite, DelegatedModule, Module, NeutralModule, RustModule};
    let catalog = crate::catalog::Catalog::builtin();
    let checks = crate::registered_checks();
    let rust = RustModule::new(&checks);
    let neutral = NeutralModule::new();
    // Under `extended` the language-neutral rules are part of what runs, so they count for every language.
    // ... and so do the delegated rules, for the tools that are installed here (a claim without its tool is a gap, not coverage).
    let delegated = DelegatedModule::new(tools.clone());
    let composite = Composite::new(vec![&rust, &neutral, &delegated]);
    let serving: &dyn Module = if with_neutral { &composite } else { &rust };
    let mut reports = Vec::new();
    if let Ok(hello) = serving.describe() {
        reports.push(for_module(catalog, &hello, "rust", Some(&result.outcomes)));
    }
    let others: Vec<_> = if with_neutral {
        neutral
            .describe()
            .into_iter()
            .chain(delegated.describe())
            .collect()
    } else {
        Vec::new()
    };
    for u in unchecked {
        reports.push(for_missing_module_with(
            catalog,
            &u.language,
            u.source_files,
            &others,
        ));
    }
    reports
}

/// The coverage lines, then (when anything is not covered) the "not yet implemented" section. The wording is deliberate: a gap is
/// CodeRipper's own incomplete coverage, not a finding about the code that was checked, and it never fails a run.
fn print_coverage_human(reports: &[crate::coverage::LanguageCoverage], full: bool) {
    for report in reports {
        println!("{}", report.human_line());
    }
    // A rule the module has built but has no conformance fixture for yet is not "not yet implemented": it runs, and it is
    // simply not counted as covered. The two lists are kept apart so neither wording is false.
    let not_built = |r: &crate::coverage::LanguageCoverage| -> Vec<String> {
        r.gaps
            .iter()
            .filter(|g| !r.claimed_unproven_ids.contains(g))
            .cloned()
            .collect()
    };
    if reports.iter().any(|r| !not_built(r).is_empty()) {
        println!(
            "Not yet implemented: CodeRipper has not built these checks yet. This says nothing about your code and never fails a run."
        );
        if full {
            for report in reports.iter().filter(|r| !not_built(r).is_empty()) {
                println!("  {}: {}", report.language, not_built(report).join(", "));
            }
        } else {
            println!("  (use --coverage=full to list them)");
        }
    }
    if reports.iter().any(|r| !r.claimed_unproven_ids.is_empty()) {
        println!(
            "Implemented, not yet proven: these checks run, but have no conformance fixture yet, so they are not counted as covered. This says nothing about your code and never fails a run."
        );
        if full {
            for report in reports
                .iter()
                .filter(|r| !r.claimed_unproven_ids.is_empty())
            {
                println!(
                    "  {}: {}",
                    report.language,
                    report.claimed_unproven_ids.join(", ")
                );
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
