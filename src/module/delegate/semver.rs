// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-001 (breaking public changes since the last release), delegated to
//! [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) (MIT OR Apache-2.0).
//!
//! **What this judges:** the crate (or, when the root `Cargo.toml` has a `[workspace]` table, every library of the workspace) at the
//! working tree against a **baseline**: the crate as a git commit has it. cargo-semver-checks builds the public interface of both (it
//! runs `cargo rustdoc`, so **the project's build scripts and procedural macros run**, and cargo may download dependencies), runs 200
//! lints for what a release must not change without a new major version (a removed function, struct, enum variant or module, a
//! changed signature, ...), and compares the verdict with the version bump: a breaking change in a release whose version is only a
//! patch (or, before 1.0, only a patch of the second number) is a failure, one released as breaking is not. The baseline is, in order:
//! the newest release tag reachable from `HEAD` (`v1.2.3` or `1.2.3`, no pre-release), else the merge-base of `HEAD` with
//! `origin/HEAD`, `origin/main` or `origin/master`. A baseline that is `HEAD` itself compares the crate with itself and says nothing
//! is broken, which is true; **no tag and no default branch is a coverage gap**, not a clean result.
//!
//! **Where it runs:** in an empty scratch directory with a scrubbed environment and `CARGO_TARGET_DIR` set into the scratch
//! directory, so nothing is written into the analysed project (measured: the tool otherwise clones the baseline below the project's
//! own `target/`, and a second run then fails with "package is ambiguous"). The project is read through `--manifest-path`.
//!
//! **The mapping record:** cargo-semver-checks prints its report on stdout (`--- failure <lint>: <title> ---`, a description, and
//! `Failed in:` lines such as `function demo::gone, previously in file <baseline checkout>/src/lib.rs:2`) and its progress on stderr;
//! it exits 100 when a lint failed and 0 when none did. One finding per failed item, **High** severity and **Medium** confidence (a
//! lint is exact about the interface, and whether a break is intended is the project's call), at the place the item was in the
//! baseline (a removed item has no place in the tree: the finding says so). An exit 100 whose report has no `Failed in:` line, or any
//! other exit code, is an error, never "no findings". Text from the tool and the project is control-character-cleaned and cut.

use super::osv::clean;
use super::{resolve, run_tool_with_env, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// The catalog rule this evidences.
pub(super) const RULE: &str = crate::rule_id::API_001;
/// The tool's name in the lock.
pub(super) const TOOL: &str = "cargo-semver-checks";

/// What the tool exits with when a lint failed (any other non-zero code is a failure of the tool).
const EXIT_BREAKING: i32 = 100;
/// What the tool exits with when it could not do what it was asked (no library, a build that failed): read for the reason.
const EXIT_ERROR: i32 = 101;
/// The refs tried, in order, for the default branch's remote copy.
const DEFAULT_BRANCHES: &[&str] = &["origin/HEAD", "origin/main", "origin/master"];

/// `git` in `dir`, with the variables that aim it at another repository cleared; stdout on success.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Whether `tag` is a release tag: `v1.2.3` or `1.2.3`, three numbers and nothing after (a pre-release is not a release).
fn is_release_tag(tag: &str) -> bool {
    let body = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Vec<&str> = body.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// What the baseline came to.
#[derive(Debug, Clone, PartialEq)]
enum Baseline {
    /// A ref to hand the tool, and how it was chosen (for the message).
    Rev { rev: String, how: String },
    /// There is none, in a way that is a coverage gap.
    Gap(String),
}

/// The baseline: the newest release tag below `HEAD`, else where the branch left the default branch.
fn baseline(root: &Path) -> Baseline {
    if git(root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).is_err() {
        return Baseline::Gap(
            "the project is not in a git repository with a commit, so there is no release to compare with".into(),
        );
    }
    if let Ok(tags) = git(
        root,
        &["tag", "--merged", "HEAD", "--sort=-version:refname"],
    ) {
        if let Some(tag) = tags.lines().map(str::trim).find(|t| is_release_tag(t)) {
            return Baseline::Rev {
                rev: tag.to_string(),
                how: format!("the release tag `{}`", clean(tag, 80)),
            };
        }
    }
    for candidate in DEFAULT_BRANCHES {
        if git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{candidate}^{{commit}}"),
            ],
        )
        .is_err()
        {
            continue;
        }
        return match git(root, &["merge-base", "HEAD", candidate]) {
            Ok(sha) => Baseline::Rev {
                rev: sha.trim().to_string(),
                how: format!("the merge-base of HEAD with {candidate} (no release tag was found)"),
            },
            Err(_) => Baseline::Gap(format!(
                "HEAD and {candidate} share no commit here (a shallow clone does not have the history), so there is no baseline; fetch more \
                 history or tag a release (`vX.Y.Z`)"
            )),
        };
    }
    Baseline::Gap(
        "the repository has no release tag (`vX.Y.Z`) below HEAD and no origin/HEAD, origin/main or origin/master to take a baseline from"
            .into(),
    )
}

/// One failed item of one lint.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    /// The lint's name (`function_missing`).
    lint: String,
    /// The lint's one-line title (`pub fn removed or renamed`).
    title: String,
    /// What failed (`function demo::gone`).
    item: String,
    /// Where it was in the baseline, relative to the crate root, with `/`.
    file: String,
    line: u32,
}

/// The path of a file in the baseline checkout relative to the baseline's root: the tool clones the baseline to
/// `<target dir>/semver-checks/git-<rev>/<hash>/`, and prints the absolute path of the file in that checkout.
fn relative(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    match parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("semver-checks"))
    {
        // `semver-checks`, `git-<rev>`, `<hash>`, then the file
        Some(i) if parts.len() > i + 3 => parts[i + 3..].join("/"),
        _ => parts.last().copied().unwrap_or_default().to_string(),
    }
}

/// Reads the report on stdout: a list of failed items grouped under `--- failure <lint>: <title> ---`. A group with no `Failed in:`
/// line is unreadable (a partial list must not look like a complete one).
fn parse_report(text: &str) -> Result<Vec<Hit>, String> {
    let mut hits = Vec::new();
    let mut current: Option<(String, String, usize)> = None; // lint, title, items found
    let mut in_failed = false;
    let close = |current: &Option<(String, String, usize)>| -> Result<(), String> {
        match current {
            Some((lint, _, 0)) => Err(format!(
                "the report lists the failure {} with no `Failed in:` item",
                clean(lint, 60)
            )),
            _ => Ok(()),
        }
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("--- failure ") {
            close(&current)?;
            let rest = rest.trim_end_matches('-').trim();
            let (lint, title) = rest.split_once(": ").unwrap_or((rest, ""));
            current = Some((lint.to_string(), title.to_string(), 0));
            in_failed = false;
            continue;
        }
        if line.trim() == "Failed in:" {
            in_failed = true;
            continue;
        }
        if !in_failed || current.is_none() {
            continue;
        }
        if line.trim().is_empty() {
            in_failed = false;
            continue;
        }
        // `  function demo::gone, previously in file <path>:<line>`
        let item_line = line.trim();
        let Some((what, place)) = item_line.rsplit_once(" in file ") else {
            return Err(format!(
                "a failed item has no place: {}",
                clean(item_line, 120)
            ));
        };
        let what = what
            .trim_end_matches("previously")
            .trim()
            .trim_end_matches(',')
            .trim();
        let Some((path, line_no)) = place.rsplit_once(':') else {
            return Err(format!(
                "a failed item has no line: {}",
                clean(item_line, 120)
            ));
        };
        let Ok(line_no) = line_no.trim().parse::<u32>() else {
            return Err(format!(
                "a failed item has no line: {}",
                clean(item_line, 120)
            ));
        };
        if let Some((lint, title, found)) = current.as_mut() {
            *found += 1;
            hits.push(Hit {
                lint: lint.clone(),
                title: title.clone(),
                item: what.to_string(),
                file: relative(path.trim()),
                line: line_no,
            });
        }
    }
    close(&current)?;
    Ok(hits)
}

/// The finding for one failed item. `how` says which baseline it was compared with.
fn to_finding(hit: &Hit, project: &str, how: &str) -> Finding {
    let item = clean(&hit.item, 200);
    let title = clean(&hit.title, 120);
    let file = clean(&hit.file, 200);
    let finding = Finding::new(
        RULE,
        Severity::High,
        Confidence::Medium,
        project,
        format!("{item}: {title}"),
        format!(
            "cargo-semver-checks ({}) reports {item} ({title}); it was at {file}:{} in the baseline ({how}), and the place is the \
             baseline's, since a removed or changed item has none now. Restore compatibility, or release the change as a breaking one: the \
             major version, and before 1.0 the second number, and say so in the changelog.",
            clean(&hit.lint, 60),
            hit.line
        ),
    )
    .location(Location::new(file.clone(), Some(hit.line)))
    .subject(item.clone());
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "cargo-semver-checks compared the crate with {how} and reported {} for {item}",
            clean(&hit.lint, 60)
        )),
    }
}

/// Whether the root manifest declares a workspace.
fn is_workspace(manifest: &Path) -> bool {
    std::fs::read_to_string(manifest)
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .is_some_and(|t| t.contains_key("workspace"))
}

/// What the whole project came to.
enum Scan {
    Read { hits: Vec<Hit>, how: String },
    Gap(String),
    Failed(ErrorKind, String),
}

fn scan(program: &Path, request: &Request) -> Scan {
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Scan::Gap(
            "the project has no Cargo.toml at its root, so there is no crate for cargo-semver-checks to compare".into(),
        );
    }
    let (rev, how) = match baseline(&root) {
        Baseline::Rev { rev, how } => (rev, how),
        Baseline::Gap(why) => return Scan::Gap(why),
    };
    // the tool clones the baseline and builds both sides below its target directory: keep that out of the project
    let target = match tempfile::tempdir() {
        Ok(t) => t,
        Err(e) => {
            return Scan::Failed(
                ErrorKind::Internal,
                format!("cannot make a scratch directory: {e}"),
            )
        }
    };
    let target_dir = target.path().to_string_lossy().to_string();
    let mut args: Vec<OsString> = [
        "semver-checks",
        "check-release",
        "--color",
        "never",
        "--manifest-path",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(manifest.clone().into_os_string());
    args.extend(["--baseline-rev".into(), OsString::from(&rev)]);
    if is_workspace(&manifest) {
        args.push("--workspace".into());
    }
    let run = match run_tool_with_env(
        program,
        |_scratch| args,
        &request.limits,
        &[EXIT_BREAKING, EXIT_ERROR],
        &[
            ("CARGO_TARGET_DIR", target_dir.as_str()),
            ("CARGO_TERM_COLOR", "never"),
        ],
    ) {
        Ok(run) => run,
        Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
    };
    let stdout = run.stdout();
    match run.exit_code() {
        Some(0) => Scan::Read {
            hits: Vec::new(),
            how,
        },
        Some(EXIT_BREAKING) => match parse_report(&stdout) {
            Ok(hits) if !hits.is_empty() => {
                if hits.len() > request.limits.max_findings {
                    return Scan::Failed(
                        ErrorKind::LimitExceeded,
                        format!(
                            "{TOOL} reported {} failed items, past the limit of {}",
                            hits.len(),
                            request.limits.max_findings
                        ),
                    );
                }
                Scan::Read { hits, how }
            }
            Ok(_) => Scan::Failed(
                ErrorKind::ToolOutputUnreadable,
                format!("{TOOL} exited {EXIT_BREAKING} but its report lists no failed item"),
            ),
            Err(why) => Scan::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}")),
        },
        // a crate with only binaries has no public interface to compare: nothing to judge, which is a gap
        Some(EXIT_ERROR) if run.log().contains("no crates with library targets") => Scan::Gap(
            "the project has no library target, so there is no public interface for cargo-semver-checks to compare (only a library has an API surface)"
                .into(),
        ),
        other => Scan::Failed(
            ErrorKind::ToolFailed,
            format!(
                "{TOOL} ended with {}: {}",
                other.map_or("no exit code".to_string(), |c| format!("code {c}")),
                clean(run.log(), 400)
            ),
        ),
    }
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    let program = match resolve(env, TOOL) {
        Resolved::Ready(program) => program,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"))
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    match scan(&program, request) {
        Scan::Read { hits, how } => {
            let root = std::path::absolute(&request.project_root)
                .unwrap_or_else(|_| request.project_root.clone());
            let project = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Verdict::Findings(hits.iter().map(|h| to_finding(h, &project, &how)).collect())
        }
        Scan::Gap(detail) => Verdict::Unavailable(detail),
        Scan::Failed(kind, detail) => Verdict::Failed(kind, detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What cargo-semver-checks 0.51.0 wrote on stdout (measured 2026-10-26, a crate whose `gone` function, `Gone` struct, `m` module
    /// and `E::B` variant were removed), with the baseline checkout's path shortened.
    const REPORT: &str = "--- failure enum_variant_missing: pub enum variant removed or renamed ---\n\nDescription:\nA publicly-visible enum has at least one variant that is no longer available under its prior name.\n        ref: https://doc.rust-lang.org/cargo/reference/semver.html#item-remove\n       impl: https://example.com/enum_variant_missing.ron\n\nFailed in:\n  variant E::B, previously in file C:\\Users\\x\\Temp\\tgt\\semver-checks\\git-v0_1_0\\839aa49b6de7c87a4c212751e8dfbea886aa0dcc\\src\\lib.rs:4\n\n--- failure function_missing: pub fn removed or renamed ---\n\nDescription:\nA publicly-visible function cannot be imported by its prior path.\n\nFailed in:\n  function demo::m::inner, previously in file /tmp/tgt/semver-checks/git-v0_1_0/839aa49b/src/lib.rs:5\n  function demo::gone, previously in file /tmp/tgt/semver-checks/git-v0_1_0/839aa49b/src/lib.rs:2\n\n";

    #[test]
    fn each_failed_item_of_each_lint_is_a_hit_at_its_place_in_the_baseline() {
        let hits = parse_report(REPORT).unwrap();
        assert_eq!(hits.len(), 3, "{hits:?}");
        assert_eq!(hits[0].lint, "enum_variant_missing");
        assert_eq!(hits[0].title, "pub enum variant removed or renamed");
        assert_eq!(hits[0].item, "variant E::B");
        assert_eq!((hits[0].file.as_str(), hits[0].line), ("src/lib.rs", 4));
        assert_eq!(hits[2].item, "function demo::gone");
        assert_eq!((hits[2].file.as_str(), hits[2].line), ("src/lib.rs", 2));
    }

    #[test]
    fn the_path_is_relative_to_the_baseline_checkout_in_either_slash_style() {
        assert_eq!(
            relative("C:\\a\\semver-checks\\git-v1\\abc\\crates\\x\\src\\lib.rs"),
            "crates/x/src/lib.rs"
        );
        assert_eq!(
            relative("/a/semver-checks/git-v1/abc/src/lib.rs"),
            "src/lib.rs"
        );
        // a path with no checkout in it keeps its file name rather than an absolute path of this machine
        assert_eq!(relative("/somewhere/else/lib.rs"), "lib.rs");
    }

    #[test]
    fn a_failure_with_no_item_or_an_item_with_no_place_is_unreadable_never_a_partial_list() {
        assert!(parse_report("--- failure x: y ---\n\nDescription:\nd\n").is_err());
        let no_place = "--- failure x: y ---\n\nFailed in:\n  function demo::a\n";
        assert!(parse_report(no_place).is_err());
        let no_line = "--- failure x: y ---\n\nFailed in:\n  function demo::a, previously in file /p/src/lib.rs\n";
        assert!(parse_report(no_line).is_err());
        assert_eq!(parse_report("").unwrap(), Vec::new());
    }

    #[test]
    fn only_three_number_tags_are_releases() {
        for yes in ["v1.2.3", "1.2.3", "v0.10.0"] {
            assert!(is_release_tag(yes), "{yes}");
        }
        for no in ["v1.2.3-rc.1", "v1.2", "release-1", "v1.2.3.4", "1.2.x", ""] {
            assert!(!is_release_tag(no), "{no}");
        }
    }

    #[test]
    fn the_finding_is_at_the_baseline_place_and_says_so() {
        let hit = Hit {
            lint: "function_missing".into(),
            title: "pub fn removed or renamed".into(),
            item: "function demo::gone".into(),
            file: "src/lib.rs".into(),
            line: 2,
        };
        let f = to_finding(&hit, "demo", "the release tag `v0.1.0`");
        assert_eq!(f.severity, Severity::High);
        assert_eq!(f.location.as_ref().unwrap().file, "src/lib.rs");
        assert_eq!(f.location.as_ref().unwrap().line, Some(2));
        assert!(f.detail.contains("in the baseline"), "{}", f.detail);
        assert_eq!(f.subject.as_deref(), Some("function demo::gone"));
    }
}
