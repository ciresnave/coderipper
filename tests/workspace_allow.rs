// SPDX-License-Identifier: MIT OR Apache-2.0
//! A waiver written once at a workspace root covers the member it names in a `--workspace` run (the root's `[[allow]]` entries are
//! inherited by each member's run, with `file` read relative to the workspace root, as in the root's own run).

mod common;

use coderipper::check::{CheckContext, Tier};
use coderipper::finding::Finding;
use coderipper::{run_workspace_in, Profile};
use common::git_repo_with;

const ROOT: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\", \"crates/b\"]\n";

fn member(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
}

/// Twelve undocumented public items: `MOD-001` reports a member's undocumented public surface at `src/lib.rs:1`.
fn undocumented() -> String {
    (0..12).map(|i| format!("pub fn f{i}() {{}}\n")).collect()
}

fn allow(file: &str) -> String {
    format!("[[allow]]\ncheck = \"MOD-001\"\nfile = \"{file}\"\nreason = \"test\"\n")
}

/// The findings of a `MOD-001` workspace run over a two-member workspace whose root file is `root_config`, and whose member files
/// are `a_config` and `b_config` (empty: none).
fn run(root_config: &str, a_config: &str, b_config: &str) -> Vec<Finding> {
    let a_src = undocumented();
    let b_src = undocumented();
    let a_manifest = member("pkg-a");
    let b_manifest = member("pkg-b");
    let mut files: Vec<(&str, &str)> = vec![
        ("Cargo.toml", ROOT),
        ("crates/a/Cargo.toml", &a_manifest),
        ("crates/a/src/lib.rs", &a_src),
        ("crates/b/Cargo.toml", &b_manifest),
        ("crates/b/src/lib.rs", &b_src),
    ];
    if !root_config.is_empty() {
        files.push((".coderipper.toml", root_config));
    }
    if !a_config.is_empty() {
        files.push(("crates/a/.coderipper.toml", a_config));
    }
    if !b_config.is_empty() {
        files.push(("crates/b/.coderipper.toml", b_config));
    }
    let repo = git_repo_with(&files);
    let ctx = CheckContext::new(repo.path().to_path_buf());
    let run = run_workspace_in(
        Profile::Extended,
        &ctx,
        Tier::Fast,
        Some("MOD-001"),
        &mut |_, _, _| {},
    );
    assert!(run.result.errors.is_empty(), "{:?}", run.result.errors);
    run.result.findings
}

fn members_of(findings: &[Finding], check: &str) -> Vec<String> {
    let mut v: Vec<String> = findings
        .iter()
        .filter(|f| f.check_id == check)
        .filter_map(|f| f.member.clone())
        .collect();
    v.sort();
    v
}

#[test]
fn positive_control_without_a_waiver_both_members_are_reported() {
    let f = run("", "", "");
    assert_eq!(members_of(&f, "MOD-001"), ["pkg-a", "pkg-b"], "{f:#?}");
}

#[test]
fn a_root_waiver_names_one_member_by_its_path_from_the_root() {
    let f = run(&allow("crates/a/src/lib.rs"), "", "");
    assert_eq!(members_of(&f, "MOD-001"), ["pkg-b"], "{f:#?}");
}

#[test]
fn a_root_waiver_for_each_member_waives_both() {
    let root = format!(
        "{}\n{}",
        allow("crates/a/src/lib.rs"),
        allow("crates/b/src/lib.rs")
    );
    let f = run(&root, "", "");
    assert!(members_of(&f, "MOD-001").is_empty(), "{f:#?}");
}

#[test]
fn a_root_waiver_is_not_reported_stale_in_a_member_it_does_not_name() {
    let f = run(&allow("crates/a/src/lib.rs"), "", "");
    assert!(
        f.iter().all(|x| x.check_id != "allowlist"),
        "an inherited entry must not be reported stale by the other member's run: {f:#?}"
    );
}

#[test]
fn a_member_path_written_without_the_member_directory_waives_nothing_at_the_root() {
    // `src/lib.rs` is the root's own path; the members' files are `crates/<m>/src/lib.rs` from the root
    let f = run(&allow("src/lib.rs"), "", "");
    assert_eq!(members_of(&f, "MOD-001"), ["pkg-a", "pkg-b"], "{f:#?}");
}

#[test]
fn a_member_file_still_waives_by_the_path_from_the_member() {
    let f = run("", &allow("src/lib.rs"), "");
    assert_eq!(members_of(&f, "MOD-001"), ["pkg-b"], "{f:#?}");
}
