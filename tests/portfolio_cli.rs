#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! `--profile portfolio`: the language-neutral rules without the delegated tool rules, failing the run by a floor per rule instead of
//! one `--deny`. The cases use a Python project (a `requirements.txt`) so only the rules under test have anything to say.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

fn project(requirements: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        ("requirements.txt", requirements),
        ("LICENSE", "MIT License\n"),
    ])
}

fn portfolio(project: &tempfile::TempDir, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(["fast", "--profile", "portfolio"])
        .args(extra)
        .arg("--project")
        .arg(project.path())
        .env("CODERIPPER_CACHE", "off")
        .env(
            "CODERIPPER_TOOLS_DIR",
            std::env::temp_dir().join("coderipper-test-no-tools-here"),
        );
    cmd
}

#[test]
fn a_project_with_nothing_the_index_lists_passes() {
    portfolio(&project("requests==2.32.0\nnumpy\n"), &[])
        .assert()
        .code(0)
        .stderr(predicate::str::contains("0 matched"));
}

#[test]
fn a_dependency_on_an_avoid_entry_blocks() {
    portfolio(&project("requests\ngensim>=4\n"), &[])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("(patent-index-match)"))
        .stdout(predicate::str::contains("index match"));
}

#[test]
fn flag_for_review_and_conditions_entries_report_but_do_not_block() {
    // openhands-ai is flag-for-review (Medium), z3-solver is allow-with-conditions (Low): both are shown, neither fails the run
    portfolio(&project("openhands-ai\nz3-solver\n"), &[])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("openhands-ai"))
        .stdout(predicate::str::contains("z3-solver"));
}

#[test]
fn deny_none_still_never_fails_on_a_finding() {
    portfolio(&project("gensim\n"), &["--deny", "none"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(patent-index-match)"));
}

#[test]
fn the_delegated_tool_rules_are_not_part_of_the_profile() {
    portfolio(&project("requests\n"), &["--help"])
        .assert()
        .success();
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(["check", "SUP-002", "--profile", "portfolio", "--project"])
        .arg(project("requests\n").path())
        .assert()
        .code(2);
}

#[test]
fn the_floors_block_mechanical_rules_and_leave_expectation_rules_advisory() {
    use coderipper::finding::Severity;
    use coderipper::portfolio_floor;
    assert_eq!(portfolio_floor("SUP-004"), Some(Severity::Medium));
    assert_eq!(portfolio_floor("conflict-marker"), Some(Severity::Low));
    assert_eq!(portfolio_floor("patent-index-match"), Some(Severity::High));
    assert_eq!(portfolio_floor("licence"), Some(Severity::High));
    // positive control above: the table answers; these are advisory
    for advisory in [
        "DOC-005",
        "DOC-009",
        "SUP-011",
        "lint-suppression",
        "reachability",
    ] {
        assert_eq!(portfolio_floor(advisory), None, "{advisory}");
    }
}

#[test]
fn a_rule_not_yet_ruled_blocking_reports_without_failing() {
    // WSP-001 (no CODEOWNERS) is a Medium finding on this project, shown and not blocking
    portfolio(&project("requests\n"), &[])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("(WSP-001)"));
}

fn with_blocking(requirements: &str, config: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        ("requirements.txt", requirements),
        ("LICENSE", "MIT License\n"),
        (".coderipper.toml", config),
    ])
}

#[test]
fn a_repository_can_make_a_rule_blocking_for_itself() {
    // WSP-001 (no CODEOWNERS) is advisory by default; this repository has opted in, so it now fails the run
    portfolio(
        &with_blocking("requests\n", "blocking = [\"WSP-001\"]\n"),
        &[],
    )
    .assert()
    .code(1)
    .stdout(predicate::str::contains("(WSP-001)"));
}

#[test]
fn blocking_names_a_rule_that_exists() {
    portfolio(
        &with_blocking("requests\n", "blocking = [\"WSP-999\"]\n"),
        &[],
    )
    .assert()
    .code(2)
    .stderr(predicate::str::contains("no rule of --profile portfolio"));
}

#[test]
fn a_repository_with_no_licence_file_blocks() {
    let no_licence = common::git_repo_with(&[(
        "requirements.txt",
        "requests
",
    )]);
    portfolio(&no_licence, &[])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("(licence)"))
        .stdout(predicate::str::contains("no licence file"));
}

#[test]
fn a_missing_header_reports_without_blocking() {
    // positive control: the same repository with no source header passes while it has a licence file; the header finding is Medium
    let repo = common::git_repo_with(&[
        (
            "requirements.txt",
            "requests
",
        ),
        ("LICENSE", "MIT License\n"),
        (
            "app.py",
            "print(1)
",
        ),
    ]);
    portfolio(&repo, &[])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("SPDX-License-Identifier"));
}

#[test]
fn an_allow_entry_for_a_rule_outside_the_profile_is_not_called_unregistered() {
    // SEC-002 is a delegated rule, which the profile leaves out; an entry naming it is for another profile's run, not a typo. A name
    // that is no rule at all still is one (the positive control).
    let p = common::git_repo_with(&[
        ("requirements.txt", "requests==2.32.0\n"),
        ("LICENSE", "MIT License\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"SEC-002\"\nfile = \"src/x.rs\"\nreason = \"history only\"\n\n[[allow]]\ncheck = \"NO-SUCH-RULE\"\nfile = \"src/y.rs\"\nreason = \"a typo\"\n",
        ),
    ]);
    portfolio(&p, &[])
        .assert()
        .stdout(predicate::str::contains(
            "`NO-SUCH-RULE`, that is not registered",
        ))
        .stdout(predicate::str::contains("`SEC-002`, that is not registered").not());
}

#[test]
fn the_list_of_registered_rules_in_an_unknown_entry_names_each_rule_once() {
    let p = common::git_repo_with(&[
        ("requirements.txt", "requests==2.32.0\n"),
        ("LICENSE", "MIT License\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"NO-SUCH-RULE\"\nfile = \"src/y.rs\"\nreason = \"a typo\"\n",
        ),
    ]);
    let out = portfolio(&p, &["--message-format", "json"])
        .assert()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    let line = out
        .lines()
        .find(|l| l.contains("NO-SUCH-RULE"))
        .expect("the unknown-entry finding");
    // the finding lists the registered rules twice (its detail and its positive control), each list naming `conflict-marker` once
    assert_eq!(line.matches("conflict-marker").count(), 2, "{line}");
}

/// A virtual workspace root (no `[package]`, two members): the layout of baracuda and fuel.
fn virtual_workspace() -> tempfile::TempDir {
    common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        ),
        ("Cargo.lock", "version = 4\n"),
        ("LICENSE", "MIT License\n"),
        (
            "a/Cargo.toml",
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("a/src/lib.rs", "pub fn f() -> i32 { 1 }\n"),
        (
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("b/src/main.rs", "fn g() -> i32 { 1 }\nfn main() { g(); }\n"),
    ])
}

fn sweep_portfolio(project: &tempfile::TempDir, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args([
        "sweep",
        "--profile",
        "portfolio",
        "--deny",
        "none",
        "--no-config",
    ])
    .args(extra)
    .arg("--project")
    .arg(project.path())
    .env("CODERIPPER_CACHE", "off");
    cmd
}

#[test]
fn a_virtual_workspace_root_runs_the_rust_checks_only_with_workspace() {
    let repo = virtual_workspace();
    // without `--workspace` the three Rust checks cannot run on the root (what portfolio.yml did before it passed the flag)
    sweep_portfolio(&repo, &[])
        .assert()
        .stderr(predicate::str::contains("virtual workspace root"));
    // with it they run on each member: the seeded defect in member `b` is found (the control that the rules ran at all)
    sweep_portfolio(&repo, &["--workspace"])
        .assert()
        .stdout(predicate::str::contains("(unused-return-values)"))
        .stderr(predicate::str::contains("virtual workspace root").not());
}

/// A workspace whose member `c` has a build script that fails unless `DOCS_RS` is set (the shape of a crate that needs nvcc).
fn workspace_with_a_member_that_needs_a_toolchain() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", "[workspace]\nmembers = [\"a\", \"c\"]\nresolver = \"2\"\n"),
        ("Cargo.lock", "version = 4\n"),
        ("LICENSE", "MIT License\n"),
        ("a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        ("a/src/lib.rs", "pub fn f() -> i32 { 1 }\n"),
        (
            "c/Cargo.toml",
            "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
        ),
        (
            "c/build.rs",
            "fn main() { if std::env::var(\"DOCS_RS\").is_err() { panic!(\"no toolchain\"); } }\n",
        ),
        ("c/src/lib.rs", "pub fn k() {}\n"),
    ])
}

#[test]
fn a_member_that_cannot_be_built_is_named_as_not_judged_and_docs_rs_lets_it_be_read() {
    let repo = workspace_with_a_member_that_needs_a_toolchain();
    // the test repository has no `origin`, so the branch-protection check is skipped: the exit code is then about the build alone
    let skip = ["--skip-rule", "ci-protection-presence=no forge in a test"];
    // without the variable the member's build fails: the run says so (exit 3) and names the member, it does not skip it quietly
    sweep_portfolio(&repo, &["--workspace", skip[0], skip[1]])
        .env_remove("DOCS_RS")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("c: reachability failed to run"))
        .stderr(predicate::str::contains("not judged"));
    // with `DOCS_RS=1` the build script skips its toolchain, the member is analysed, and nothing is reported as not judged
    sweep_portfolio(&repo, &["--workspace", skip[0], skip[1]])
        .env("DOCS_RS", "1")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("not judged").not());
}

#[test]
fn a_package_whose_targets_are_outside_src_says_so_instead_of_a_bare_os_error() {
    // baracuda's `examples` package: only `[[bin]]` entries with a `path` at the package root, no `src/`
    let repo = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[package]
name = \"ex\"
version = \"0.0.0\"
edition = \"2021\"
publish = false

[[bin]]
name = \"hello\"
path = \"hello.rs\"
",
        ),
        (
            "Cargo.lock",
            "version = 4
",
        ),
        (
            "LICENSE",
            "MIT License
",
        ),
        (
            "hello.rs",
            "fn main() {}
",
        ),
    ]);
    sweep_portfolio(
        &repo,
        &["--skip-rule", "ci-protection-presence=no forge in a test"],
    )
    .assert()
    .code(3)
    .stderr(predicate::str::contains("has no `src/` directory"))
    .stderr(predicate::str::contains("os error").not());
}

fn workspace_with_config(config: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]
members = [\"a\", \"b\"]
resolver = \"2\"
",
        ),
        (
            "Cargo.lock",
            "version = 4
",
        ),
        (
            "LICENSE",
            "MIT License
",
        ),
        (".coderipper.toml", config),
        (
            "a/Cargo.toml",
            "[package]
name = \"a\"
version = \"0.1.0\"
edition = \"2021\"
",
        ),
        (
            "a/src/lib.rs",
            "pub fn f() -> i32 { 1 }
",
        ),
        (
            "b/Cargo.toml",
            "[package]
name = \"b\"
version = \"0.1.0\"
edition = \"2021\"
",
        ),
        (
            "b/src/lib.rs",
            "pub fn g() -> i32 { 1 }
",
        ),
    ])
}

#[test]
fn a_crate_level_public_api_declaration_covers_one_package_of_a_workspace() {
    let skip = ["--skip-rule", "ci-protection-presence=no forge in a test"];
    // `a` and `b` each have an unreferenced public function; the declaration names only `a`
    let repo = workspace_with_config(
        "[reachability]
public_api_crates = [{ crate = \"a\", reason = \"published library\" }]
",
    );
    sweep_portfolio(&repo, &["--workspace", skip[0], skip[1]])
        .assert()
        .stdout(predicate::str::contains(
            "[Medium/Medium] b — `g` has zero callers",
        ))
        .stdout(predicate::str::contains("[Medium/Medium] a — `f`").not())
        .stdout(predicate::str::contains(
            "[declared-public/Medium] a — `f` has zero callers",
        ));
    // a misspelt package name declares nothing and says so by name
    let repo = workspace_with_config(
        "[reachability]
public_api_crates = [{ crate = \"aa\", reason = \"published library\" }]
",
    );
    sweep_portfolio(&repo, &["--workspace", skip[0], skip[1]])
        .assert()
        .stdout(predicate::str::contains(
            "[Medium/Medium] a — `f` has zero callers",
        ))
        .stdout(predicate::str::contains("public_api_crates"))
        .stdout(predicate::str::contains("`aa`"));
}

#[test]
fn a_member_run_by_hand_agrees_with_the_workspace_run_on_a_root_only_declaration() {
    // the lanes reproduce CI member by member (`--project crates/x`); a run that disagrees with CI is a defect
    let repo = workspace_with_config(
        "[reachability]
public_api_crates = [{ crate = \"a\", reason = \"published library\" }]
",
    );
    let skip = ["--skip-rule", "ci-protection-presence=no forge in a test"];
    let at = |dir: &str| {
        let mut cmd = Command::cargo_bin("coderipper").unwrap();
        cmd.args([
            "sweep",
            "--profile",
            "portfolio",
            "--deny",
            "none",
            "--no-config",
        ])
        .args(skip)
        .arg("--project")
        .arg(repo.path().join(dir))
        .env("CODERIPPER_CACHE", "off");
        cmd
    };
    // `a` is declared at the root: its run reads the root's file and prints the finding as declared public
    at("a")
        .assert()
        .stdout(predicate::str::contains(
            "[declared-public/Medium] a — `f` has zero callers",
        ))
        .stdout(predicate::str::contains("[Medium/Medium] a — `f`").not());
    // `b` is not declared: it keeps its finding, and is not told that another package's entry matched nothing
    at("b")
        .assert()
        .stdout(predicate::str::contains(
            "[Medium/Medium] b — `g` has zero callers",
        ))
        .stdout(predicate::str::contains("public_api_crates").not());
}
