//! Golden files: the two formats other programs and other projects depend on, pinned byte for byte.
//!
//! - `tests/golden/finding.json`: how a `Finding` serializes (what `--message-format json` is built from, and what a
//!   tool reading it parses).
//! - `tests/golden/coderipper.toml`: the `.coderipper.toml` format, with every key it has. A project's allowlist is
//!   written once and read by every later version, so a renamed key would silently stop suppressing.
//!
//! The golden files are written by hand, not generated from the code: a test that compared the code with its own output
//! could not fail.

use coderipper::check::{Check, CheckContext, Network, Scope, Tier};
use coderipper::finding::{Confidence, Finding, Location, Severity};

const GOLDEN_FINDING: &str = include_str!("golden/finding.json");
const GOLDEN_ALLOWLIST: &str = include_str!("golden/coderipper.toml");

fn the_finding() -> Finding {
    Finding::new(
        "unused-return-values",
        Severity::Medium,
        Confidence::High,
        "widgets",
        "`render`'s return value is discarded at every call site",
        "Nothing reads what `render` returns.",
    )
    .location(Location::new("src/lib.rs", Some(12)))
    .subject("render")
    .member("widgets-core")
    .positive_control("the same query finds `draw`, whose result is read")
}

#[test]
fn a_finding_serializes_exactly_as_the_golden_file() {
    let written = serde_json::to_string_pretty(&the_finding()).unwrap();
    assert_eq!(
        written.trim(),
        GOLDEN_FINDING.replace("\r\n", "\n").trim(),
        "the JSON shape of a Finding changed: that is a breaking change for everything that reads it"
    );
}

#[test]
fn the_golden_finding_reads_back_into_the_same_finding() {
    let read: Finding = serde_json::from_str(GOLDEN_FINDING).unwrap();
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        serde_json::to_value(the_finding()).unwrap()
    );
}

#[test]
fn a_finding_without_a_member_leaves_the_key_out() {
    let mut value = serde_json::to_value(the_finding()).unwrap();
    value.as_object_mut().unwrap().remove("member");
    let read: Finding = serde_json::from_value(value).unwrap();
    assert!(read.member.is_none());
    assert!(!serde_json::to_string(&read).unwrap().contains("member"));
}

/// Reports `TODO.md`, which the golden allowlist's `[[allow]]` entry names.
struct TodoFile;

impl Check for TodoFile {
    fn id(&self) -> &'static str {
        "todo-file"
    }
    fn scope(&self) -> Scope {
        Scope::Project
    }
    fn network(&self) -> Network {
        Network::LocalOnly
    }
    fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        Ok(vec![Finding::new(
            "todo-file",
            Severity::Low,
            Confidence::High,
            "demo",
            "the project keeps a TODO.md",
            "d",
        )
        .location(Location::new("TODO.md", None))
        .subject("TODO.md")])
    }
}

fn run_with_allowlist(contents: &str) -> coderipper::RunResult {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".coderipper.toml"), contents).unwrap();
    let checks: Vec<Box<dyn Check>> = vec![Box::new(TodoFile)];
    coderipper::run_checks_with(
        &checks,
        &CheckContext::new(dir.path(), dir.path()),
        Tier::Fast,
        None,
    )
}

#[test]
fn the_golden_allowlist_loads_and_its_allow_entry_suppresses() {
    let result = run_with_allowlist(GOLDEN_ALLOWLIST);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        result.findings.is_empty(),
        "the [[allow]] entry (check, file, symbol) must suppress the finding: {:?}",
        result.findings
    );
}

#[test]
fn an_allow_entry_without_a_reason_is_refused() {
    let without_reason: String = GOLDEN_ALLOWLIST
        .lines()
        .filter(|l| !l.starts_with("reason = \"kept on purpose\""))
        .collect::<Vec<_>>()
        .join("\n");
    let result = run_with_allowlist(&without_reason);
    assert!(
        result.errors.iter().any(|e| e.contains("reason")),
        "an allowlist entry must say why: {:?}",
        result.errors
    );
}

#[test]
fn a_misspelled_key_is_an_error_not_a_silently_ignored_entry() {
    let misspelled = GOLDEN_ALLOWLIST.replace("symbol =", "symbl =");
    let result = run_with_allowlist(&misspelled);
    assert!(!result.errors.is_empty(), "{:?}", result.findings);
}
