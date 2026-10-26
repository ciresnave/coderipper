#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The result cache (audit row C7, the incremental cache): a second run over the same committed tree is served from the store and
//! reports exactly what the first run computed; a different tree, or the switch turned off, computes again.

mod common;

use assert_cmd::Command;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const HIT: &str = "result cache - 1 reused (same committed tree)";

/// A project with one Medium finding from `unused-return-values` (`f`'s result is always discarded).
fn project(extra: &str) -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/main.rs",
            &format!("fn f() -> i32 {{ 1 }}\nfn main() {{ f(); }}\n{extra}"),
        ),
    ])
}

/// One classic run of the check, with the build cache (and so the result cache) in `cache`. Returns stdout and stderr.
fn run(project: &std::path::Path, cache: &std::path::Path, result_cache: &str) -> (String, String) {
    let out = Command::cargo_bin("coderipper")
        .unwrap()
        .env("CODERIPPER_CACHE_DIR", cache)
        .env("CODERIPPER_CACHE", "on")
        .env("CODERIPPER_RESULT_CACHE", result_cache)
        .args([
            "check",
            "unused-return-values",
            "--profile",
            "classic",
            "--deny",
            "none",
            "--project",
        ])
        .arg(project)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn a_second_run_over_the_same_tree_reports_the_same_findings_and_says_it_reused_them() {
    let repo = project("");
    let cache = tempfile::tempdir().unwrap();
    let (first, first_err) = run(repo.path(), cache.path(), "on");
    assert!(
        first.contains("(unused-return-values)"),
        "{first}\n{first_err}"
    );
    assert!(
        !first_err.contains(HIT),
        "the first run computes: {first_err}"
    );
    let (second, second_err) = run(repo.path(), cache.path(), "on");
    assert_eq!(second, first, "a reused result is the computed one");
    assert!(second_err.contains(HIT), "{second_err}");
}

#[test]
fn a_different_tree_or_the_switch_off_computes_again() {
    let repo = project("");
    let cache = tempfile::tempdir().unwrap();
    run(repo.path(), cache.path(), "on");
    // the positive control above stores an entry; with the switch off it is not read
    let (_, off_err) = run(repo.path(), cache.path(), "off");
    assert!(!off_err.contains(HIT), "{off_err}");
    // another repository with another tree: a miss
    let other = project("fn g() {}\n");
    let (_, other_err) = run(other.path(), cache.path(), "on");
    assert!(!other_err.contains(HIT), "{other_err}");
}
