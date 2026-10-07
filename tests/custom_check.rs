//! A user's own `Check`, run through the host with `run_checks_with`.
//!
//! This is an outside crate, so it is exactly what a library user can do. The behaviours pinned here are the ones the
//! docs must state: the host validates findings, loads the allowlist from the project, judges stale entries against the
//! checks YOU pass, and keeps the tier rule.

use coderipper::check::{Check, CheckContext, Network, Scope, Tier};
use coderipper::finding::{Confidence, Finding, Location, Severity};
use coderipper::run_checks_with;
use std::path::Path;

/// Reports a `TODO.md` in the project root.
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
    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        if !ctx.project_root.join("TODO.md").exists() {
            return Ok(Vec::new());
        }
        Ok(vec![Finding::new(
            "todo-file",
            Severity::Low,
            Confidence::High,
            "demo",
            "the project keeps a TODO.md",
            "TODO.md is for the issue tracker",
        )
        .location(Location::new("TODO.md", None))
        .subject("TODO.md")])
    }
}

/// A check that claims an absence and gives no positive control: the host must reject it.
struct Careless;

impl Check for Careless {
    fn id(&self) -> &'static str {
        "careless"
    }
    fn scope(&self) -> Scope {
        Scope::Project
    }
    fn network(&self) -> Network {
        Network::LocalOnly
    }
    fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        Ok(vec![Finding::new(
            "careless",
            Severity::High,
            Confidence::High,
            "demo",
            "no callers",
            "nothing calls it",
        )])
    }
}

/// A network-tier (sweep) check.
struct SweepOnly;

impl Check for SweepOnly {
    fn id(&self) -> &'static str {
        "sweep-only"
    }
    fn scope(&self) -> Scope {
        Scope::Project
    }
    fn network(&self) -> Network {
        Network::NetworkRequired
    }
    fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        Ok(vec![Finding::new(
            "sweep-only",
            Severity::Info,
            Confidence::High,
            "demo",
            "ran in the sweep tier",
            "d",
        )])
    }
}

fn project_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).unwrap();
    }
    dir
}

fn ctx(dir: &Path) -> CheckContext {
    CheckContext::new(dir)
}

fn boxed(check: impl Check + 'static) -> Vec<Box<dyn Check>> {
    vec![Box::new(check)]
}

#[test]
fn a_users_own_check_runs_and_its_finding_comes_back() {
    let dir = project_with(&[("TODO.md", "- later\n")]);
    let result = run_checks_with(&boxed(TodoFile), &ctx(dir.path()), Tier::Fast, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].check_id, "todo-file");
    assert_eq!(result.findings[0].subject.as_deref(), Some("TODO.md"));
}

#[test]
fn only_the_checks_you_pass_run() {
    // an empty directory is not a cargo project: the built-in checks would all fail here, so a clean result proves they
    // were not run
    let dir = project_with(&[]);
    let result = run_checks_with(&boxed(TodoFile), &ctx(dir.path()), Tier::Fast, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty());
}

#[test]
fn the_host_validates_your_findings() {
    let dir = project_with(&[]);
    let result = run_checks_with(&boxed(Careless), &ctx(dir.path()), Tier::Fast, None);
    assert!(
        result.findings.is_empty(),
        "an invalid finding must not reach a report"
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("careless") && e.contains("positive_control")),
        "{:?}",
        result.errors
    );
}

#[test]
fn the_projects_allowlist_suppresses_a_finding_that_names_it() {
    let dir = project_with(&[
        ("TODO.md", "- later\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"todo-file\"\nfile = \"TODO.md\"\nsymbol = \"TODO.md\"\nreason = \"kept on purpose\"\n",
        ),
    ]);
    let result = run_checks_with(&boxed(TodoFile), &ctx(dir.path()), Tier::Fast, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        result.findings.is_empty(),
        "an entry matching check_id + location.file + subject suppresses: {:?}",
        result.findings
    );
}

#[test]
fn an_entry_that_matches_nothing_is_reported_stale() {
    let dir = project_with(&[
        ("TODO.md", "- later\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"todo-file\"\nfile = \"TODO.md\"\nsymbol = \"some-other-symbol\"\nreason = \"was true once\"\n",
        ),
    ]);
    let result = run_checks_with(&boxed(TodoFile), &ctx(dir.path()), Tier::Fast, None);
    assert!(
        result.findings.iter().any(|f| f.check_id == "todo-file"),
        "not suppressed"
    );
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.check_id == "allowlist" && f.severity == Severity::Info),
        "the stale entry is reported: {:?}",
        result.findings
    );
}

#[test]
fn an_entry_naming_a_check_you_did_not_pass_is_reported_as_unknown() {
    // The documented gotcha: stale and unknown entries are judged against the checks YOU pass. If the project's
    // .coderipper.toml names a built-in check and you pass only your own, that entry is an "unknown check".
    let dir = project_with(&[(
        ".coderipper.toml",
        "[[allow]]\ncheck = \"reachability\"\nfile = \"src/lib.rs\"\nsymbol = \"f\"\nreason = \"kept\"\n",
    )]);
    let result = run_checks_with(&boxed(TodoFile), &ctx(dir.path()), Tier::Fast, None);
    let unknown = result
        .findings
        .iter()
        .find(|f| f.check_id == "allowlist")
        .expect("an unknown-check finding");
    assert!(
        unknown.summary.contains("reachability"),
        "{}",
        unknown.summary
    );
}

#[test]
fn a_sweep_tier_check_runs_in_the_fast_tier_only_when_named() {
    let dir = project_with(&[]);
    let sweep = boxed(SweepOnly);
    let fast = run_checks_with(&sweep, &ctx(dir.path()), Tier::Fast, None);
    assert!(
        fast.findings.is_empty(),
        "a sweep check must not run in the fast tier"
    );
    let named = run_checks_with(&sweep, &ctx(dir.path()), Tier::Fast, Some("sweep-only"));
    assert_eq!(named.findings.len(), 1, "naming it runs it at any tier");
    let in_sweep = run_checks_with(&sweep, &ctx(dir.path()), Tier::Sweep, None);
    assert_eq!(in_sweep.findings.len(), 1);
}

/// Implements only what a check must: no `scope`, because nothing reads it.
struct Minimal;

impl Check for Minimal {
    fn id(&self) -> &'static str {
        "minimal"
    }
    fn network(&self) -> Network {
        Network::LocalOnly
    }
    fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        Ok(Vec::new())
    }
}

#[test]
fn a_check_need_not_declare_a_scope() {
    let dir = project_with(&[]);
    let result = run_checks_with(&boxed(Minimal), &ctx(dir.path()), Tier::Fast, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        Minimal.scope(),
        Scope::Project,
        "the default is the project"
    );
}

#[test]
fn naming_a_check_that_does_not_exist_is_an_error_not_a_clean_run() {
    // A typo in a CI step must not be green forever: "no findings" and "nothing ran" are different answers.
    let dir = project_with(&[]);
    let result = run_checks_with(
        &boxed(TodoFile),
        &ctx(dir.path()),
        Tier::Fast,
        Some("todo-fiel"),
    );
    assert!(result.findings.is_empty());
    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    assert!(
        result.errors[0].contains("no check named \"todo-fiel\""),
        "{}",
        result.errors[0]
    );
    assert!(
        result.errors[0].contains("todo-file"),
        "the error lists the checks that exist: {}",
        result.errors[0]
    );
}

#[test]
fn naming_a_check_that_exists_is_still_fine() {
    let dir = project_with(&[("TODO.md", "- later\n")]);
    let result = run_checks_with(
        &boxed(TodoFile),
        &ctx(dir.path()),
        Tier::Fast,
        Some("todo-file"),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.findings.len(), 1);
}

#[test]
fn two_checks_with_one_id_are_refused() {
    // The id is the key of allowlist entries and of `only_check_id`; two checks sharing it would make both ambiguous.
    let dir = project_with(&[("TODO.md", "- later\n")]);
    let twice: Vec<Box<dyn Check>> = vec![Box::new(TodoFile), Box::new(TodoFile)];
    let result = run_checks_with(&twice, &ctx(dir.path()), Tier::Fast, None);
    assert!(
        result.findings.is_empty(),
        "nothing runs: {:?}",
        result.findings
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("todo-file") && e.contains("more than one check")),
        "{:?}",
        result.errors
    );
}

#[test]
fn a_workspace_run_with_an_unknown_check_id_says_so_instead_of_visiting_every_member() {
    let dir = project_with(&[]);
    let run = coderipper::run_workspace(
        &ctx(dir.path()),
        Tier::Fast,
        Some("nope"),
        &mut |_, _, _| {},
    );
    assert_eq!(run.members, 0);
    assert!(
        run.result
            .errors
            .iter()
            .any(|e| e.contains("no check named \"nope\"")),
        "{:?}",
        run.result.errors
    );
}
