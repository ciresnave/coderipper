//! SEC-002 (no committed secrets), delegated to [gitleaks](https://github.com/gitleaks/gitleaks) (MIT).
//!
//! **What this judges:** gitleaks scans the repository's whole git history (`gitleaks git`), so a secret that was committed and
//! later deleted is still reported: it stays readable to anyone with a clone. What it matches is gitleaks' own rule set at the
//! version the lock pins; a secret in a format that rule set does not know is not seen, and the rule says nothing about secrets
//! outside git (an untracked `.env`, a build log).
//!
//! **What is read:** gitleaks' JSON report, written to a file in a scratch directory, with the secrets redacted by gitleaks itself
//! (`--redact`): the value of a secret is never read into CodeRipper, so it cannot reach a finding, a log or the JSON output.
//! gitleaks' exit code is not the verdict (`--exit-code 0`; a non-zero exit is a failure of the tool).
//!
//! **The mapping record:** every gitleaks `RuleID` becomes a SEC-002 finding at `File`:`StartLine`, subject the file, severity
//! `High` (a committed credential is exploitable by anyone who can read the repository). Confidence is capped at `Medium`
//! until this module's own fixtures say more (design section 7), and is `Low` for the generic rule, which matches by entropy
//! near a name and is the one that fires on non-secrets.

use super::{resolve, run_tool, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The catalog rule this evidences.
pub(super) const RULE: &str = "SEC-002";
/// The tool's name in the lock.
pub(super) const TOOL: &str = "gitleaks";

/// The mapping from gitleaks' `RuleID` to what is reported. A `RuleID` not in this table gets [`DEFAULT_CONFIDENCE`].
const CONFIDENCE: &[(&str, Confidence)] = &[("generic-api-key", Confidence::Low)];
const DEFAULT_CONFIDENCE: Confidence = Confidence::Medium;

/// One entry of gitleaks' JSON report (only the fields used; the redacted `Secret` and `Match` are deliberately not read).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Leak {
    #[serde(rename = "RuleID")]
    rule_id: String,
    #[serde(default)]
    description: String,
    file: String,
    #[serde(default)]
    start_line: u32,
    #[serde(default)]
    commit: String,
}

/// Reads gitleaks' JSON report. Anything but a JSON array of leaks is unreadable; an empty array is a clean scan.
fn parse_report(text: &str) -> Result<Vec<Leak>, String> {
    serde_json::from_str::<Vec<Leak>>(text).map_err(|e| {
        format!(
            "the report is not a JSON array of gitleaks findings ({e}): {}",
            text.chars().take(200).collect::<String>()
        )
    })
}

/// The finding for one leak.
fn to_finding(leak: &Leak, project: &str) -> Finding {
    let file = leak.file.replace('\\', "/");
    let confidence = CONFIDENCE
        .iter()
        .find(|(id, _)| *id == leak.rule_id)
        .map_or(DEFAULT_CONFIDENCE, |(_, c)| *c);
    let short: String = leak.commit.chars().take(8).collect();
    let line = (leak.start_line > 0).then_some(leak.start_line);
    let finding = Finding::new(
        RULE,
        Severity::High,
        confidence,
        project,
        format!("a committed secret (gitleaks rule {})", leak.rule_id),
        format!(
            "gitleaks rule {} matched at {file}{} in commit {short}: {} The value is redacted on purpose. A secret in history stays \
             readable to anyone with a clone, so revoke and replace it first; rewriting history comes second.",
            leak.rule_id,
            line.map_or(String::new(), |l| format!(":{l}")),
            if leak.description.is_empty() {
                "a secret."
            } else {
                leak.description.as_str()
            },
        ),
    )
    .location(Location::new(file.clone(), line))
    .subject(file.clone());
    // The summary is built from the rule id, which could in principle contain a whole word the host reads as an absence claim;
    // such a finding carries a control rather than being dropped.
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "gitleaks scanned the history and reported rule {} at {file} in commit {short}",
            leak.rule_id
        )),
    }
}

/// The configuration gitleaks is run with, written to the scratch directory: its default rules and nothing the analysed repository
/// supplies. (Left alone, gitleaks reads `.gitleaks.toml` and `.gitleaksignore` from the repository it scans, so the project could
/// silence its own findings; the project's way to suppress a finding is the `.coderipper.toml` allowlist, which is visible.)
const CONFIG: &str = "[extend]\nuseDefault = true\n";

/// The arguments of the scan: history of the repository at `repo`, the report and the configuration in `scratch`, secrets
/// redacted, exit code not the verdict, and no `gitleaks:allow` comment, `.gitleaksignore` or `.gitleaks.toml` of the repository
/// honoured. The source is last and must be absolute (the tool runs in `scratch`, and an absolute path cannot be read as a flag). It
/// is the repository's **git directory**, not its working tree: gitleaks looks for `.gitleaksignore` in the source it is given
/// whatever `--gitleaks-ignore-path` says, and a `.git` directory holds no committed file.
fn scan_args(git_dir: &Path, scratch: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "git",
        "--no-banner",
        "--no-color",
        "--redact",
        "--exit-code",
        "0",
        "--ignore-gitleaks-allow",
        "--report-format",
        "json",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    for (flag, file) in [
        ("--report-path", "report.json"),
        ("--config", "config.toml"),
        ("--gitleaks-ignore-path", "gitleaksignore"),
    ] {
        args.push(flag.into());
        args.push(scratch.join(file).into());
    }
    args.push(git_dir.into());
    args
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    let program = match resolve(env, TOOL) {
        Resolved::Ready(path) => path,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"))
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    scan_with(&program, scan_args, request)
}

/// `git <args>` in `dir`: its output when it succeeded (only the line ending is trimmed: a path may begin with a space).
fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    crate::conformance::git_command(dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_end_matches(['\n', '\r'])
                .to_string()
        })
}

/// What gitleaks' log says about whether it really read the history, or `None` when it did.
///
/// gitleaks exits 0 with an empty report when the `git` it runs fails (a repository git calls unsafe, say): `ERR` lines and
/// "0 commits scanned". That is a scan that did not happen, and must not read as a clean rule. The log is not the verdict, only a
/// check on it: it can turn "nothing found" into a failure, never the reverse. `expected` is the number of commits in the repository.
fn log_problem(log: &str, expected: usize) -> Option<String> {
    if let Some(line) = log.lines().find(|l| l.contains(" ERR ")) {
        return Some(format!("it logged an error: {}", line.trim()));
    }
    let scanned = log.lines().find_map(|l| {
        let (before, _) = l.split_once(" commits scanned")?;
        before.rsplit(' ').next()?.parse::<usize>().ok()
    });
    match scanned {
        None => Some("its log does not say how many commits it scanned".to_string()),
        Some(0) if expected > 0 => Some(format!(
            "it scanned 0 commits of the {expected} in the repository"
        )),
        Some(_) => None,
    }
}

/// Runs `program` with the arguments `args(repo, scratch)` builds, and reads the JSON report it was told to write at
/// `scratch/report.json`. (The arguments are a parameter so a test can stand in a program that misbehaves; the tool is always run
/// with [`scan_args`].)
pub(super) fn scan_with(
    program: &Path,
    args: fn(&Path, &Path) -> Vec<OsString>,
    request: &Request,
) -> Verdict {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and scan nothing.
    let repo =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    // gitleaks scans an empty history, and says so only in its log, when there is none: that must not read as a clean rule.
    if git_output(&repo, &["rev-parse", "--verify", "HEAD"]).is_none() {
        return Verdict::Failed(
            ErrorKind::ToolFailed,
            format!(
                "{} is not a git repository with a commit; {RULE} reads its history",
                repo.display()
            ),
        );
    }
    let Some(git_dir) = git_output(&repo, &["rev-parse", "--absolute-git-dir"]) else {
        return Verdict::Failed(
            ErrorKind::ToolFailed,
            format!("cannot find the git directory of {}", repo.display()),
        );
    };
    let git_dir = PathBuf::from(git_dir);
    let run = match run_tool(
        program,
        |scratch| {
            // a failure to write these surfaces as the tool failing to start with them
            let _ = std::fs::write(scratch.join("config.toml"), CONFIG);
            let _ = std::fs::write(scratch.join("gitleaksignore"), "");
            args(&git_dir, scratch)
        },
        &request.limits,
    ) {
        Ok(run) => run,
        Err((kind, detail)) => return Verdict::Failed(kind, format!("{TOOL}: {detail}")),
    };
    let report = run.scratch().join("report.json");
    let text = match std::fs::read(&report) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => {
            return Verdict::Failed(
                ErrorKind::ToolOutputUnreadable,
                format!("{TOOL} wrote no report ({e})"),
            )
        }
    };
    let mut leaks = match parse_report(&text) {
        Ok(leaks) => leaks,
        Err(why) => {
            return Verdict::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}"))
        }
    };
    let expected = git_output(&repo, &["rev-list", "--all", "--count"])
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(1);
    if let Some(why) = log_problem(run.log(), expected) {
        return Verdict::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} did not read the history ({why}); its report says nothing"),
        );
    }
    if leaks.len() > request.limits.max_findings {
        return Verdict::Failed(
            ErrorKind::LimitExceeded,
            format!(
                "{TOOL} reported {} findings, past the limit of {}",
                leaks.len(),
                request.limits.max_findings
            ),
        );
    }
    // gitleaks scans the whole repository whatever directory it is given, and names files from the repository root. A project in a
    // subdirectory keeps the leaks under it, named relative to it, so a finding's location and the allowlist agree with the project.
    let prefix = git_output(&repo, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    if !prefix.is_empty() {
        leaks = leaks
            .into_iter()
            .filter_map(|mut leak| {
                let file = leak.file.replace('\\', "/");
                leak.file = file.strip_prefix(prefix.as_str())?.to_string();
                Some(leak)
            })
            .collect();
    }
    let project = repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // A shallow clone (CI's default checkout) holds only the commits it fetched: an empty report there is about those commits, not
    // about the history, so it must not read as a clean rule. Findings, if there are any, are true whatever else is hidden.
    if leaks.is_empty()
        && git_output(&repo, &["rev-parse", "--is-shallow-repository"]).as_deref() == Some("true")
    {
        return Verdict::Unavailable(
            "the repository is a shallow clone, so gitleaks saw only the commits that were fetched and an empty report says nothing \
             about the rest of the history; fetch all of it (git fetch --unshallow) to check this rule"
                .into(),
        );
    }
    Verdict::Findings(leaks.iter().map(|l| to_finding(l, &project)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What gitleaks 8.30.1 wrote for a one-commit repository holding `vendor_api_key = "..."` (measured 2026-10-09 with `--redact`).
    const REPORT: &str = r#"[
 {
  "RuleID": "generic-api-key",
  "Description": "Detected a Generic API Key, potentially exposing access to various services and sensitive operations.",
  "StartLine": 1,
  "EndLine": 1,
  "StartColumn": 1,
  "EndColumn": 50,
  "Match": "vendor_api_key = \"REDACTED\"",
  "Secret": "REDACTED",
  "File": "settings.cfg",
  "SymlinkFile": "",
  "Commit": "2fdc11483a2424d4bd488334c15eb3221d521abf",
  "Entropy": 5.121928,
  "Author": "t",
  "Email": "a@b.c",
  "Date": "2026-10-09T08:11:14Z",
  "Message": "init",
  "Tags": [],
  "Fingerprint": "2fdc11483a2424d4bd488334c15eb3221d521abf:settings.cfg:generic-api-key:1"
 }
]"#;

    #[test]
    fn the_real_report_shape_maps_to_a_sec_002_finding_at_the_leak() {
        let leaks = parse_report(REPORT).unwrap();
        assert_eq!(leaks.len(), 1);
        let f = to_finding(&leaks[0], "demo");
        assert_eq!(f.check_id, "SEC-002");
        assert_eq!(f.severity, Severity::High);
        assert_eq!(
            f.confidence,
            Confidence::Low,
            "the generic rule is the noisy one"
        );
        assert_eq!(f.project, "demo");
        let location = f.location.as_ref().unwrap();
        assert_eq!(
            (location.file.as_str(), location.line),
            ("settings.cfg", Some(1))
        );
        assert_eq!(f.subject.as_deref(), Some("settings.cfg"));
        assert_eq!(
            f.summary,
            "a committed secret (gitleaks rule generic-api-key)"
        );
        assert!(f.detail.contains("2fdc1148"), "{}", f.detail);
        assert!(f.clone().validate().is_ok());
    }

    #[test]
    fn a_specific_rule_is_medium_confidence_and_the_secret_never_appears() {
        let text = REPORT
            .replace("generic-api-key", "aws-access-token")
            .replace("REDACTED", "AKIAREALLOOKINGVALUE");
        let leaks = parse_report(&text).unwrap();
        let f = to_finding(&leaks[0], "demo");
        assert_eq!(f.confidence, Confidence::Medium);
        let all = format!("{f:?}");
        assert!(
            !all.contains("AKIAREALLOOKINGVALUE"),
            "the secret was read into a finding: {all}"
        );
    }

    #[test]
    fn a_rule_id_with_an_absence_word_still_makes_a_valid_finding_with_a_control() {
        let mut leaks = parse_report(REPORT).unwrap();
        leaks[0].rule_id = "none".into(); // a whole word: the host would read the summary as an absence claim
        let f = to_finding(&leaks[0], "demo");
        assert!(f.positive_control.is_some());
        assert!(f.validate().is_ok());
    }

    #[test]
    fn the_scan_ignores_whatever_the_repository_says_about_itself() {
        let args: Vec<String> = scan_args(Path::new("/abs/repo/.git"), Path::new("/scratch"))
            .iter()
            .map(|a| a.to_string_lossy().replace('\\', "/"))
            .collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        assert!(has("--redact"), "{args:?}");
        assert!(has("--ignore-gitleaks-allow"), "{args:?}");
        assert!(has("--config") && has("--gitleaks-ignore-path"), "{args:?}");
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("--exit-code"), "0");
        assert_eq!(after("--config"), "/scratch/config.toml");
        assert_eq!(after("--gitleaks-ignore-path"), "/scratch/gitleaksignore");
        assert_eq!(args.last().map(String::as_str), Some("/abs/repo/.git"));
        assert_eq!(args[0], "git");
    }

    #[test]
    fn a_log_that_shows_the_history_was_not_read_is_a_failure_not_a_clean_scan() {
        // measured with gitleaks 8.30.1: a healthy scan, and git failing inside gitleaks (exit code 0, report `[]`)
        let healthy = "2:07AM INF 2 commits scanned.\n2:07AM INF scanned ~214 bytes (214 bytes) in 556ms\n2:07AM INF no leaks found\n";
        let unsafe_repo = "2:07AM ERR [git] fatal: detected dubious ownership in repository at 'C:/x'\n2:07AM ERR error=\"stderr is not empty\"\n2:07AM INF 0 commits scanned.\n2:07AM INF no leaks found\n";
        let nothing_scanned = "2:07AM INF 0 commits scanned.\n2:07AM INF no leaks found\n";
        assert_eq!(log_problem(healthy, 2), None);
        assert!(log_problem(unsafe_repo, 2)
            .unwrap()
            .contains("dubious ownership"));
        assert!(log_problem(nothing_scanned, 2)
            .unwrap()
            .contains("0 commits"));
        assert!(log_problem("", 2).unwrap().contains("does not say"));
        assert!(log_problem("garbage", 2).is_some());
    }

    #[test]
    fn windows_separators_in_a_path_become_slashes() {
        let mut leaks = parse_report(REPORT).unwrap();
        leaks[0].file = r"conf\prod\settings.cfg".into();
        let f = to_finding(&leaks[0], "demo");
        assert_eq!(f.location.unwrap().file, "conf/prod/settings.cfg");
    }

    #[test]
    fn an_empty_array_is_a_clean_scan_and_anything_else_is_unreadable() {
        assert_eq!(parse_report("[]").unwrap().len(), 0);
        for bad in ["", "null", "{}", "not json", r#"[{"File":"a"}]"#, "[1]"] {
            assert!(
                parse_report(bad).is_err(),
                "{bad:?} must be unreadable, not clean"
            );
        }
    }
}
