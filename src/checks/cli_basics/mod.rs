// SPDX-License-Identifier: MIT OR Apache-2.0
//! CLI-basics check (project scope, Rust): does every binary answer `--version`, `-V`, `--help`, `-h` and a bad flag, and nothing else?
//!
//! Rule record: `OPS-011` in `rules/ops.toml` (the rule catalog). Background in `docs/rules/ops.md`.
//!
//! The standard: `--version` and `-V` print the crate version and exit 0; `--help` and `-h` print usage and exit 0; an unknown flag
//! prints a short error on stderr (stdout empty) and exits 2. No output carries ANSI escape sequences (it is a pipe). None of the five starts work: no home or configuration lookup, no file or directory created,
//! no server started, no hang. It exists because `synapsed --version` once tried to start the daemon and failed on a store path
//! instead of printing a version.
//!
//! **How:** the package is built (`cargo build --bins`) from the committed tree in a throwaway worktree, and each binary cargo wrote
//! is run through the five probes in `probe` (a fresh sandbox per probe, an empty environment, a 10-second timeout, the sandbox
//! listed before and after). One finding per binary that misbehaves, naming each probe that failed and why. A probe that times out
//! is run once more before it is believed, and makes the finding Medium (the binary started something that does not end); every
//! other failure is Low. The rule is advisory.
//!
//! **Known limitations / deliberate scope:**
//! - Not probed (follow-up rows): a missing-file argument (a binary's file arguments are unknown), `$EDITOR`/`$VISUAL`, config dotfiles in the home root, `NO_COLOR`, stdin `-`, `-y`/`-q`/`-v`, Ctrl-C. Platform-aware by construction: the probe points `HOME`/`USERPROFILE`/`APPDATA`/`LOCALAPPDATA` and the `XDG_*` roots all into the sandbox, so a write under any of them is seen on either OS.
//! - Rust binaries only. A `package.json` `bin` entry or a Python console script is not run: this check says nothing about them.
//! - Only what the binary does under the sandbox is seen. A write outside it (its own directory, an absolute path, `/var`) or a
//!   network call is not; a daemon the binary forks and leaves running past the kill is not waited for.
//! - A binary that needs a terminal from its first line, or an argument before it will parse flags, fails here: record that in the
//!   project's `.coderipper.toml` as a `[[cli_waiver]]` (`bin`, `probes`, a required `reason`): the waived probe is printed as
//!   `waived: <reason>` and the binary's other probes are still judged.
//! - A `[[bin]]` whose `required-features` are off is not built and so not run (cargo reports no executable for it); the
//!   `--features` this run was given are passed to the build.
//! - The probes run the project's own build output on the machine running CodeRipper: this is the one check that executes the
//!   built binaries, not only the build scripts (`executes_project_code` in the record).
//! - Analysis is HEAD-only, single-package projects only, as with the other checks.

mod probe;

use crate::allowlist::{Allowlist, CliWaiver};
use crate::cargo_json::build_bins;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use std::path::Path;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::OPS_011;

/// Runs every binary through the standard flags (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct CliBasicsCheck;

impl CliBasicsCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for CliBasicsCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        // The rewrite is the identity: this check runs what was committed, it does not change it.
        let wt =
            RewrittenWorktree::create_with(&ctx.project_root, |_, source| Ok(source.to_string()))?;
        let (package, _) = crate::package::locate(&wt.root)?;
        let build = build_bins(&wt.root, Some(wt.source_repo()), &ctx.cargo_features)?;
        anyhow::ensure!(
            !build.is_broken(),
            "this crate's build reported real compiler error(s) (or failed without any diagnostic), \
             so no binary was built and no result can be trusted. Cause: {}",
            build.failure_cause()
        );
        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let allowlist = Allowlist::load(&ctx.project_root)?;
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        for waiver in allowlist.cli_waivers() {
            if !build.executables.iter().any(|b| b.name == waiver.bin) {
                notes.push(format!(
                    "`[[cli_waiver]]` names `{}`, which is no binary this run built; it waives nothing",
                    waiver.bin
                ));
            }
        }
        for bin in &build.executables {
            let all = probe::probe_all(&bin.path, &package.version, probe::TIMEOUT);
            let (violations, waived) = split_waived(all, &bin.name, allowlist.cli_waivers());
            for (v, reason) in &waived {
                notes.push(format!(
                    "waived: `{} {}` ({}): {reason}",
                    bin.name, v.args, v.problem
                ));
            }
            for waiver in allowlist.cli_waivers().iter().filter(|w| w.bin == bin.name) {
                for name in &waiver.probes {
                    if !waived.iter().any(|(v, _)| probe_name(&v.args) == name) {
                        notes.push(format!(
                            "`[[cli_waiver]]` for `{}` waives probe `{name}`, which passed: the waiver is stale",
                            bin.name
                        ));
                    }
                }
            }
            if violations.is_empty() {
                continue;
            }
            let hung = violations.iter().any(|v| v.hung);
            let list = violations
                .iter()
                .map(|v| format!("`{} {}`: {}", bin.name, v.args, v.problem))
                .collect::<Vec<_>>()
                .join("\n");
            let file = relative_to(&wt.root, &bin.src_path);
            findings.push(
                Finding::new(
                    CHECK_ID,
                    if hung { Severity::Medium } else { Severity::Low },
                    Confidence::High,
                    project.clone(),
                    format!(
                        "binary `{}` fails {} of {} CLI-basics probes",
                        bin.name,
                        violations.len(),
                        probe::PROBE_COUNT
                    ),
                    format!(
                        "Each probe ran the built binary in an empty directory with an empty environment (10 s timeout). \
                         Expected: `--version`/`-V` print the crate version ({}) and exit 0, `--help`/`-h` print usage and exit 0, \
                         an unknown flag exits 2 with its message on stderr and nothing on stdout, no output carries ANSI escapes, and none of them creates a file or directory or starts anything.\n{list}",
                        package.version
                    ),
                )
                .location(Location::new(file, None))
                .subject(bin.name.clone()),
            );
        }
        findings.sort_by(|a, b| a.subject.cmp(&b.subject));
        Ok((findings, notes))
    }
}

/// The name a `[[cli_waiver]]` gives a probe: the flag as typed, or `unknown-flag` for the bad-flag probe.
fn probe_name(args: &str) -> &str {
    if args == probe::UNKNOWN_FLAG {
        "unknown-flag"
    } else {
        args
    }
}

/// `violations` split into those that still count and those a `[[cli_waiver]]` for `bin` waives, each with its reason.
fn split_waived<'a>(
    violations: Vec<probe::Violation>,
    bin: &str,
    waivers: &'a [CliWaiver],
) -> (Vec<probe::Violation>, Vec<(probe::Violation, &'a str)>) {
    let mut kept = Vec::new();
    let mut waived = Vec::new();
    for v in violations {
        let waiver = waivers
            .iter()
            .find(|w| w.bin == bin && w.probes.iter().any(|p| p == probe_name(&v.args)));
        match waiver {
            Some(w) => waived.push((v, w.reason.as_str())),
            None => kept.push(v),
        }
    }
    (kept, waived)
}

/// `src` (an absolute path cargo printed) relative to the package directory `root`, forward slashes; `src` as given when it is
/// not below `root`.
fn relative_to(root: &Path, src: &str) -> String {
    let (Ok(root), Ok(src_path)) = (std::fs::canonicalize(root), std::fs::canonicalize(src)) else {
        return src.to_string();
    };
    src_path
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| src.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violation(args: &str) -> probe::Violation {
        probe::Violation {
            args: args.to_string(),
            problem: "exit 1, want 2".to_string(),
            hung: false,
        }
    }

    fn waiver(bin: &str, probes: &[&str]) -> CliWaiver {
        CliWaiver {
            bin: bin.to_string(),
            probes: probes.iter().map(|p| p.to_string()).collect(),
            reason: "usage error exits 1 by contract".to_string(),
        }
    }

    #[test]
    fn a_waived_probe_is_set_aside_with_its_reason_and_the_others_still_count() {
        let all = vec![violation("--version"), violation(probe::UNKNOWN_FLAG)];
        let waivers = [waiver("lane-restart", &["unknown-flag"])];
        let (kept, waived) = split_waived(all, "lane-restart", &waivers);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].args, "--version");
        assert_eq!(waived.len(), 1);
        assert_eq!(waived[0].0.args, probe::UNKNOWN_FLAG);
        assert_eq!(waived[0].1, "usage error exits 1 by contract");
    }

    #[test]
    fn a_waiver_for_another_binary_waives_nothing() {
        let waivers = [waiver("other", &["--version"])];
        let (kept, waived) = split_waived(vec![violation("--version")], "lane-restart", &waivers);
        assert_eq!(kept.len(), 1);
        assert!(waived.is_empty());
    }
}
