//! [`DelegatedModule`]: the rules CodeRipper hands to a tool it installs and runs as an external process (design section 7).
//!
//! A delegated rule has a **mapping record**: the tool's own code for a finding (gitleaks' `RuleID`, say) to the catalog rule it
//! evidences, with the severity, confidence, location, subject and summary to report. Everything a rule needs from the framework
//! is here, so the next tool is one file:
//!
//! - **Getting the tool.** [`resolve`] turns a name into an absolute path through the [`ToolEnv`]. A tool that is not there
//!   (not pinned, no build for this platform, not installed and not allowed to be) is a **coverage gap, never a failure**: the
//!   rule's result is [`RuleResult::tool_unavailable`], the run goes on, and the message names the command that would install it.
//!   A tool that was asked for and could not be trusted or had (a checksum that differs, an archive that is refused, an install that
//!   failed after the user said to install it) is an error: the audit did not happen and says so.
//! - **Running it.** [`run_tool`] starts it with a scrubbed environment, an empty scratch directory as its working directory and
//!   the run's limits, and kills the process tree on a timeout. It never runs inside the analysed project.
//! - **Reading it.** Only the tool's machine-readable output is read (gitleaks: its JSON report). Output that cannot be read is
//!   `tool_output_unreadable`, never "no findings".
//!
//! The module is in process (the tool is the child), so a rule's findings belong to the result that follows them.

mod buf;
mod gitleaks;
mod lychee;
mod osv;
mod zizmor;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::external::{run_child_accepting, Who, ENV_ALLOWLIST};
use super::{
    Capabilities, ErrorKind, Event, Hello, Limits, Module, ModuleOutput, ModuleSummary, Request,
    RuleClaim, RuleResult,
};
use crate::finding::Finding;
use crate::tools::{ToolEnv, ToolError};

/// What one delegated rule concluded.
pub(crate) enum Verdict {
    /// The tool ran and its output was read: these are the findings (none means the rule is clean).
    Findings(Vec<Finding>),
    /// The rule cannot be judged here (a gap, never a failure): its tool is not available, or what the tool would see is not the
    /// whole of what the rule is about (a shallow clone). The detail says what to do about it.
    Unavailable(String),
    /// The rule gave no verdict.
    Failed(ErrorKind, String),
}

/// What [`resolve`] found.
pub(crate) enum Resolved {
    /// The tool, by absolute path.
    Ready(PathBuf),
    /// Absent, in a way that is a gap (see the module documentation).
    Unavailable(String),
    /// Not obtainable in a way the user should hear about as an error.
    Failed(ErrorKind, String),
}

/// The absolute path of `tool`, or why there is none.
pub(crate) fn resolve(env: &ToolEnv, tool: &str) -> Resolved {
    match env.resolve(tool) {
        Ok(path) => Resolved::Ready(path),
        Err(
            e @ (ToolError::Missing(_)
            | ToolError::UnavailableForPlatform { .. }
            | ToolError::ConsentNotGiven { .. }),
        ) => Resolved::Unavailable(e.to_string()),
        Err(e @ ToolError::ChecksumMismatch { .. }) => {
            Resolved::Failed(ErrorKind::ChecksumMismatch, e.to_string())
        }
        // the user asked for the install and it did not happen: the tool is missing and they should hear it as an error
        Err(
            e @ (ToolError::DownloadFailed { .. }
            | ToolError::InstallFailed { .. }
            | ToolError::ArchiveRefused { .. }),
        ) => Resolved::Failed(ErrorKind::ToolMissing, e.to_string()),
        Err(e @ ToolError::LockInvalid(_)) => {
            Resolved::Failed(ErrorKind::ConfigInvalid, e.to_string())
        }
    }
}

/// What a tool run produced: the files it was told to write sit in `scratch`, which lives as long as this value.
pub(crate) struct ToolRun {
    scratch: tempfile::TempDir,
    stderr: String,
    exit_code: Option<i32>,
    stdout: Vec<String>,
}

impl ToolRun {
    /// The scratch directory the tool ran in and wrote its report to.
    pub(crate) fn scratch(&self) -> &Path {
        self.scratch.path()
    }

    /// The tail of the tool's stderr (its log), for a tool whose exit code does not say whether it saw everything.
    pub(crate) fn log(&self) -> &str {
        &self.stderr
    }

    /// What the tool printed on stdout, for a tool that has no option to write its report to a file (zizmor).
    pub(crate) fn stdout(&self) -> String {
        self.stdout.join("\n")
    }

    /// The exit code the tool ended with (one of the `ok_codes` given to [`run_tool`], or 0).
    pub(crate) fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

/// Runs `program` with `args` and the run's limits. `args` may name files in the scratch directory: `args` is given that
/// directory, so the tool's report path can be built before it starts.
///
/// An exit code other than 0 or one of `ok_codes` (the codes this tool uses to answer, such as "found something"), a timeout,
/// output past the limit: each is `Err((kind, detail))` with the tail of the tool's stderr.
pub(crate) fn run_tool(
    program: &Path,
    args: impl FnOnce(&Path) -> Vec<std::ffi::OsString>,
    limits: &Limits,
    ok_codes: &[i32],
) -> Result<ToolRun, (ErrorKind, String)> {
    let scratch = tempfile::tempdir().map_err(|e| {
        (
            ErrorKind::Internal,
            format!("cannot make a scratch directory: {e}"),
        )
    })?;
    let mut command = Command::new(program);
    command
        .args(args(scratch.path()))
        .env_clear()
        .current_dir(scratch.path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let run = run_child_accepting(command, None, limits, Who::Tool, ok_codes);
    match run.failure {
        Some(failure) => Err((failure.kind, failure.detail)),
        None => Ok(ToolRun {
            scratch,
            stderr: run.stderr,
            exit_code: run.exit_code,
            stdout: run.lines,
        }),
    }
}

/// The tracked files of the repository at `root`, NUL-separated, from the index (never a walk of the disk: another worktree or a
/// virtualenv below `root` is not the project's).
pub(crate) fn tracked_listing(root: &Path) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|e| format!("cannot run git to list the tracked files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git could not list the tracked files: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One delegated rule: its catalog id, the tool it runs, and the function that runs it.
struct Rule {
    id: &'static str,
    tool: &'static str,
    run: fn(&ToolEnv, &Request) -> Verdict,
}

const RULES: &[Rule] = &[
    Rule {
        id: gitleaks::RULE,
        tool: gitleaks::TOOL,
        run: gitleaks::run,
    },
    Rule {
        id: osv::SUP,
        tool: osv::TOOL,
        run: osv::run_sup,
    },
    Rule {
        id: osv::SEC,
        tool: osv::TOOL,
        run: osv::run_sec,
    },
    Rule {
        id: lychee::RULE,
        tool: lychee::TOOL,
        run: lychee::run,
    },
    Rule {
        id: zizmor::RULE,
        tool: zizmor::TOOL,
        run: zizmor::run,
    },
    Rule {
        id: buf::RULE,
        tool: buf::TOOL,
        run: buf::run,
    },
];

/// The rules this module delegates, in the order it reports them.
pub fn delegated_rule_ids() -> Vec<&'static str> {
    RULES.iter().map(|r| r.id).collect()
}

/// The rules handed to installed tools. It answers the module protocol in process; the tools are its children.
///
/// What it claims it can *prove*: a claim carries its conformance proof only while the tool is installed here, so a machine
/// without the tool reports the rule as a gap in the coverage figure instead of as covered. (The proof is a fixture pair run
/// against the real tool by `coderipper conformance --module delegated --install-tools`.)
///
/// Inside a [`super::Composite`] with an external module the whole output counts as not in process, and a tool gap is then
/// refused as a protocol mismatch: compose built-in modules only.
#[derive(Debug, Clone)]
pub struct DelegatedModule {
    env: ToolEnv,
    /// Tools seen installed once: describing a module is cheap after that (installed files are hashed when first seen, and again
    /// each time a rule resolves its tool).
    seen_installed: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl DelegatedModule {
    /// The module over these tools.
    pub fn new(env: ToolEnv) -> Self {
        Self {
            env,
            seen_installed: std::sync::Arc::default(),
        }
    }

    /// Whether `tool` is installed here and matches the lock; a positive answer is remembered (hashing a tool is not free).
    fn installed(&self, tool: &str) -> bool {
        let mut seen = self
            .seen_installed
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if seen.iter().any(|t| t == tool) {
            return true;
        }
        let now = self.env.is_installed(tool);
        if now {
            seen.push(tool.to_string());
        }
        now
    }
}

impl Default for DelegatedModule {
    /// The embedded lock, the environment's tools directory, and no consent to install.
    fn default() -> Self {
        Self::new(ToolEnv::from_environment())
    }
}

impl Module for DelegatedModule {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "delegated",
            env!("CARGO_PKG_VERSION"),
            vec!["any".into()],
            vec![],
            // the tools read the repository (a child process); none runs the project's code. osv-scanner's and lychee's rules need the network
            // (they are sweep-tier rules, and a run that does not permit it reports them as a gap)
            Capabilities::new(false, true, false, false),
            RULES
                .iter()
                .map(|r| {
                    let mut claim = RuleClaim::native(r.id);
                    claim.status = "delegated".into();
                    claim.tool = Some(r.tool.into());
                    if self.installed(r.tool) {
                        claim.proof = Some(format!("conformance/{}", r.id));
                    }
                    claim
                })
                .collect(),
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let mut events = Vec::new();
        let mut summary = ModuleSummary {
            rules_requested: request.rules.len(),
            ..ModuleSummary::default()
        };
        for rule in &request.rules {
            let Some(known) = RULES.iter().find(|r| r.id == rule) else {
                events.push(Event::RuleResult(RuleResult::error(
                    rule,
                    ErrorKind::Internal,
                    "the delegated module has no such rule",
                )));
                summary.rules_errored += 1;
                continue;
            };
            match (known.run)(&self.env, request) {
                Verdict::Findings(found) => {
                    summary.rules_ran += 1;
                    summary.findings += found.len();
                    let count = found.len();
                    events.extend(found.into_iter().map(|f| Event::Finding(Box::new(f))));
                    events.push(Event::RuleResult(RuleResult::ran(rule, count)));
                }
                Verdict::Unavailable(detail) => {
                    summary.rules_skipped += 1;
                    events.push(Event::RuleResult(RuleResult::tool_unavailable(
                        rule, detail,
                    )));
                }
                Verdict::Failed(kind, detail) => {
                    summary.rules_errored += 1;
                    events.push(Event::RuleResult(RuleResult::error(rule, kind, detail)));
                }
            }
        }
        events.push(Event::Summary(summary));
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }
}

#[cfg(test)]
mod tests;
