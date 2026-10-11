#![cfg(feature = "cli")]
//! A repository with no Rust crate at its root (a web app whose backend is in `backend/`, a site with no Rust at all).
//!
//! Before 0.6.0 every Rust check asked cargo about the root and failed, so a repository of any other shape exited 3 with
//! four "could not run" errors and was never audited. Now: no `Cargo.toml` at the root is "no Rust crate at this path" (a
//! coverage gap, never a failure); crates found below the root are checked where they are, and the run says which; and a
//! crate that is found and cannot be built is still an error, never a skip.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use predicates::prelude::*;

/// A manifest for a crate named `name`.
fn manifest(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
}

/// One Medium `unused-return-values` finding: `f`'s result is always discarded.
const DEFECTIVE_MAIN: &str = "fn f() -> i32 { 1 }\nfn main() { f(); }\n";

fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

fn run(args: &[&str], project: &std::path::Path) -> assert_cmd::assert::Assert {
    coderipper()
        .args(args)
        .arg("--project")
        .arg(project)
        .assert()
}

fn stderr_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stderr).into_owned()
}

fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8_lossy(&assert.get_output().stdout).into_owned()
}

#[test]
fn a_repository_with_no_rust_exits_0_and_says_there_is_no_crate() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n"), ("index.html", "<p>x</p>\n")]);
    let out = run(&["fast"], repo.path()).code(0);
    let err = stderr_of(&out);
    assert!(err.contains("no Rust crate at this path"), "{err}");
    assert!(!err.contains("check error"), "{err}");
    assert!(!err.contains("cargo metadata"), "{err}");
}

#[test]
fn no_rust_is_counted_as_not_run_here_in_the_coverage_line() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["fast", "--coverage"], repo.path()).code(0);
    let text = stdout_of(&out);
    // reachability, unused-return-values, unused-parameters, version-consistency: not applicable, not failed
    assert!(
        text.contains("0 clean, 0 with findings, 4 not run here, 0 could not run")
            || text.contains(", 4 not run here, 0 could not run"),
        "{text}"
    );
}

#[test]
fn a_rule_named_on_a_repository_with_no_rust_is_a_gap_not_a_failure() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["check", "reachability"], repo.path()).code(0);
    assert!(stderr_of(&out).contains("no Rust crate at this path"));
}

#[test]
fn json_summary_carries_the_gap_as_a_note_and_no_errors() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["fast", "--message-format", "json"], repo.path()).code(0);
    let text = stdout_of(&out);
    let summary = text
        .lines()
        .find(|l| l.contains("coderipper-summary"))
        .expect("a summary line");
    let v: serde_json::Value = serde_json::from_str(summary).unwrap();
    assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{summary}");
    assert!(
        v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("no Rust crate at this path")),
        "{summary}"
    );
}

#[test]
fn a_crate_below_the_root_is_found_and_checked() {
    let repo = common::git_repo_with(&[
        ("README.md", "# app\n"),
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(&["fast"], repo.path()).code(1);
    let text = stdout_of(&out);
    assert!(text.contains("(unused-return-values)"), "{text}");
    // the finding leads with the package, as a `--workspace` finding does
    assert!(text.contains("backendcrate"), "{text}");
    let err = stderr_of(&out);
    assert!(
        err.contains("backend/"),
        "the run must say where it found the crate: {err}"
    );
    assert!(!err.contains("check error"), "{err}");
}

#[test]
fn deny_none_prints_a_discovered_crates_finding_and_exits_0() {
    let repo = common::git_repo_with(&[
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
    ]);
    run(&["fast", "--deny", "none"], repo.path())
        .code(0)
        .stdout(predicate::str::contains("(unused-return-values)"));
}

#[test]
fn a_clean_nested_crate_is_clean_and_counted_as_run() {
    let repo = common::git_repo_with(&[
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", "fn main() {}\n"),
    ]);
    let out = run(&["fast", "--coverage"], repo.path()).code(0);
    let text = stdout_of(&out);
    assert!(
        text.contains("4 clean, 0 with findings, 0 not run here, 0 could not run"),
        "{text}"
    );
}

#[test]
fn a_nested_crate_that_does_not_build_is_an_error_and_never_a_skip() {
    let repo = common::git_repo_with(&[
        ("README.md", "# app\n"),
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", "this is not valid rust {{{\n"),
    ]);
    let out = run(&["fast"], repo.path()).code(3);
    let err = stderr_of(&out);
    assert!(err.contains("check error"), "{err}");
    assert!(
        err.contains("backendcrate"),
        "the error names the crate: {err}"
    );
}

#[test]
fn two_unrelated_crates_are_both_checked_and_both_named() {
    let repo = common::git_repo_with(&[
        ("a/Cargo.toml", &manifest("alpha")),
        ("a/src/main.rs", DEFECTIVE_MAIN),
        ("b/Cargo.toml", &manifest("beta")),
        ("b/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(&["fast", "--deny", "none"], repo.path()).code(0);
    let text = stdout_of(&out);
    assert!(text.contains("alpha") && text.contains("beta"), "{text}");
    let err = stderr_of(&out);
    assert!(err.contains("a/") && err.contains("b/"), "{err}");
}

#[test]
fn a_nested_virtual_workspace_is_checked_member_by_member() {
    let repo = common::git_repo_with(&[
        (
            "services/Cargo.toml",
            "[workspace]\nmembers = [\"api\"]\nresolver = \"2\"\n",
        ),
        ("services/api/Cargo.toml", &manifest("apicrate")),
        ("services/api/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(&["fast", "--deny", "none"], repo.path()).code(0);
    let text = stdout_of(&out);
    assert!(text.contains("apicrate"), "{text}");
    assert!(text.contains("(unused-return-values)"), "{text}");
    assert!(!stderr_of(&out).contains("check error"));
}

#[test]
fn a_named_check_on_a_nested_crate_runs_only_that_check() {
    let repo = common::git_repo_with(&[
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
    ]);
    run(&["check", "unused-return-values"], repo.path())
        .code(1)
        .stdout(predicate::str::contains("(unused-return-values)"));
    run(&["check", "version-consistency"], repo.path()).code(0);
}

#[test]
fn a_root_crate_is_untouched_by_discovery() {
    // a crate at the root plus an unrelated crate below it: the root is the project, nothing is "discovered", and the
    // run does not mention discovery
    let repo = common::git_repo_with(&[
        ("Cargo.toml", &manifest("rootcrate")),
        ("src/main.rs", "fn main() {}\n"),
        ("tools/helper/Cargo.toml", &manifest("helper")),
        ("tools/helper/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(&["fast"], repo.path()).code(0);
    let err = stderr_of(&out);
    assert!(!err.contains("no Rust crate"), "{err}");
    assert!(!err.contains("found Rust crate"), "{err}");
    assert!(!stdout_of(&out).contains("helper"));
}

#[test]
fn workspace_mode_without_a_root_crate_discovers_too() {
    let repo = common::git_repo_with(&[
        ("README.md", "# app\n"),
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(&["fast", "--workspace", "--deny", "none"], repo.path()).code(0);
    assert!(stdout_of(&out).contains("(unused-return-values)"));
    assert!(!stderr_of(&out).contains("check error"));
}

#[test]
fn workspace_mode_with_no_rust_is_a_gap_too() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["fast", "--workspace"], repo.path()).code(0);
    assert!(stderr_of(&out).contains("no Rust crate at this path"));
}

#[test]
fn a_rule_that_is_not_about_rust_still_runs_where_there_is_no_crate() {
    // ci-protection-presence reads the forge, not a crate: it must not be skipped with the Rust rules. With no origin it
    // reports an ERROR, which proves it ran (a skip would have been silent).
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["check", "ci-protection-presence"], repo.path()).code(3);
    assert!(stderr_of(&out).contains("ci-protection-presence"));
}

#[test]
fn an_uncommitted_crate_at_the_root_is_an_error_never_a_skip() {
    // `Cargo.toml` exists on disk but git does not track it: the checks read HEAD and cannot see it. Saying "no Rust crate"
    // would be false (and silent); cargo's own error is the truth.
    let repo = common::git_repo_with(&[("README.md", "# app\n")]);
    std::fs::write(repo.path().join("Cargo.toml"), manifest("late")).unwrap();
    let out = run(&["fast"], repo.path()).code(3);
    let err = stderr_of(&out);
    assert!(err.contains("check error"), "{err}");
    assert!(!err.contains("no Rust crate at this path"), "{err}");
}

#[test]
fn a_project_directory_inside_a_crate_is_not_a_rootless_repository() {
    // `--project app/examples/web`: no manifest there, but the repository tracks `app/Cargo.toml` above it, so cargo has an
    // answer and the host must not replace it with "no Rust crate at this path"
    let repo = common::git_repo_with(&[
        ("app/Cargo.toml", &manifest("app")),
        ("app/src/main.rs", "fn main() {}\n"),
        ("app/examples/web/index.html", "<p>x</p>\n"),
    ]);
    let out = run(&["fast"], &repo.path().join("app/examples/web"));
    assert!(
        !stderr_of(&out).contains("no Rust crate at this path"),
        "{}",
        stderr_of(&out)
    );
}

#[test]
fn the_note_counts_a_nested_workspace_once_however_many_manifests_it_has() {
    let repo = common::git_repo_with(&[
        (
            "services/Cargo.toml",
            "[workspace]\nmembers = [\"api\", \"worker\"]\nresolver = \"2\"\n",
        ),
        ("services/api/Cargo.toml", &manifest("apicrate")),
        ("services/api/src/main.rs", "fn main() {}\n"),
        ("services/worker/Cargo.toml", &manifest("workercrate")),
        ("services/worker/src/main.rs", "fn main() {}\n"),
    ]);
    let out = run(&["fast"], repo.path()).code(0);
    let err = stderr_of(&out);
    assert!(err.contains("found 2 Rust crate(s)"), "{err}");
    assert!(err.contains("apicrate (services/api/Cargo.toml)"), "{err}");
}

#[test]
fn a_crates_own_waiver_file_applies_to_its_findings() {
    let repo = common::git_repo_with(&[
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
        (
            "backend/.coderipper.toml",
            "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nreason = \"fixture: discarded on purpose\"\n",
        ),
    ]);
    let out = run(&["fast"], repo.path()).code(0);
    let text = stdout_of(&out);
    assert!(text.contains("[waived/"), "{text}");
    assert!(text.contains("discarded on purpose"), "{text}");
}

#[test]
fn only_the_named_check_runs_on_a_nested_crate() {
    // the crate has a finding for BOTH rules; naming one must print only that one
    let repo = common::git_repo_with(&[
        ("backend/Cargo.toml", &manifest("backendcrate")),
        (
            "backend/src/main.rs",
            "fn f() -> i32 { 1 }\npub fn g(unused: i32) {}\nfn main() { f(); g(1); }\n",
        ),
    ]);
    let out = run(
        &["check", "unused-return-values", "--deny", "none"],
        repo.path(),
    )
    .code(0);
    let text = stdout_of(&out);
    assert!(text.contains("(unused-return-values)"), "{text}");
    assert!(!text.contains("(unused-parameters)"), "{text}");
}

#[test]
fn extended_profile_runs_the_neutral_rules_once_and_the_crates_beside_them() {
    let repo = common::git_repo_with(&[
        ("README.md", "# app\n"),
        ("backend/Cargo.toml", &manifest("backendcrate")),
        ("backend/src/main.rs", DEFECTIVE_MAIN),
    ]);
    let out = run(
        &[
            "fast",
            "--profile",
            "extended",
            "--deny",
            "none",
            "--message-format",
            "json",
        ],
        repo.path(),
    )
    .code(0);
    let text = stdout_of(&out);
    let count = |id: &str| {
        text.lines()
            .filter(|l| {
                l.contains("coderipper-finding") && l.contains(&format!("\"check_id\":\"{id}\""))
            })
            .count()
    };
    assert_eq!(count("WSP-001"), 1, "the repository rule runs once: {text}");
    assert_eq!(count("unused-return-values"), 1, "{text}");
}

#[test]
fn the_coverage_list_does_not_call_an_implemented_rule_not_yet_implemented() {
    let repo = common::git_repo_with(&[("README.md", "# a site\n")]);
    let out = run(&["fast", "--coverage=full"], repo.path()).code(0);
    let text = stdout_of(&out);
    // `ci-protection-presence` is built and has no conformance fixture yet (it needs a forge): it is listed apart
    let not_built = text
        .lines()
        .skip_while(|l| !l.starts_with("Not yet implemented"))
        .take_while(|l| !l.starts_with("Implemented"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!not_built.contains("ci-protection-presence"), "{text}");
    let proven_later = text
        .lines()
        .skip_while(|l| !l.starts_with("Implemented, not yet proven"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(proven_later.contains("ci-protection-presence"), "{text}");
}
