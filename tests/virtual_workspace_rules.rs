#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The source-reading module rules (MOD-001..005, MOD-008, MOD-009, ARC-008) run per member on a virtual workspace root
//! (a `[workspace]` with no `[package]`: Fuel, baracuda, Unpopped) instead of refusing it with "not a package directory".

mod common;

use assert_cmd::Command;
use predicates::prelude::*;

const RULES: [&str; 8] = [
    "MOD-001", "MOD-002", "MOD-003", "MOD-004", "MOD-005", "MOD-008", "MOD-009", "ARC-008",
];

/// A virtual workspace root with two members: `clean` forwards with a change of its own, `wrapper` has a pure pass-through
/// (the MOD-008 seeded defect). Neither is the other's twin by accident: the rule must say which member has the wrapper.
fn virtual_workspace() -> tempfile::TempDir {
    let manifest = |name: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    };
    common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"clean\", \"wrapper\"]\nresolver = \"2\"\n",
        ),
        ("Cargo.lock", "version = 4\n"),
        ("clean/Cargo.toml", &manifest("clean")),
        (
            "clean/src/lib.rs",
            "mod util;\n\npub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
        ),
        (
            "clean/src/util.rs",
            "pub fn plus(a: u8, b: u8) -> u8 {\n    crate::add(a, b).saturating_add(1)\n}\n",
        ),
        ("wrapper/Cargo.toml", &manifest("wrapper")),
        (
            "wrapper/src/lib.rs",
            "mod util;\n\npub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n",
        ),
        (
            "wrapper/src/util.rs",
            "pub fn plus(a: u8, b: u8) -> u8 {\n    crate::add(a, b)\n}\n",
        ),
    ])
}

fn check(rule: &str, project: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(["--no-config", "check", rule, "--deny", "none", "--project"])
        .arg(project.path())
        .env("CODERIPPER_CACHE", "off");
    cmd
}

#[test]
fn every_source_reading_module_rule_runs_on_a_virtual_workspace_root() {
    let repo = virtual_workspace();
    for rule in RULES {
        check(rule, &repo)
            .assert()
            .stderr(predicate::str::contains("not a package directory").not())
            .stderr(predicate::str::contains("failed to run").not());
    }
}

#[test]
fn the_seeded_wrapper_is_found_in_the_member_that_has_it_and_only_there() {
    let repo = virtual_workspace();
    let out = check("MOD-008", &repo).assert().get_output().clone();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        stdout.contains("`plus` only passes its arguments on to `crate::add`")
            || stdout.contains("only passes its arguments on"),
        "{stdout}"
    );
    assert_eq!(
        stdout.matches("(MOD-008)").count(),
        1,
        "exactly the defective member is reported: {stdout}"
    );
    assert!(stdout.contains("wrapper"), "{stdout}");
    assert!(
        !stdout.contains("clean —") && !stdout.contains("clean -"),
        "{stdout}"
    );
}

#[test]
fn a_directory_that_is_neither_a_package_nor_the_workspace_root_is_still_refused() {
    let repo = virtual_workspace();
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args([
        "--no-config",
        "check",
        "MOD-008",
        "--deny",
        "none",
        "--project",
    ])
    .arg(repo.path().join("clean").join("src"))
    .env("CODERIPPER_CACHE", "off");
    cmd.assert()
        .stderr(predicate::str::contains("not a package directory"));
}
