#![cfg(feature = "cli")]
//! `--workspace`: every member of a cargo workspace in one command, through the real CLI.
//!
//! Fixtures are tempdir git repositories with tiny crates. Most runs switch the build cache off (the session then builds
//! in its own temporary target); the payoff test uses a cache directory per run.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use std::collections::BTreeSet;
use std::path::Path;

struct Out {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn cli(args: &[&str], envs: &[(&str, &str)]) -> Out {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.args(args);
    // these tests are about what is printed, not the exit code: a finding must not fail the run
    if args.iter().any(|a| ["fast", "sweep", "check"].contains(a)) && !args.contains(&"--deny") {
        cmd.args(["--deny", "none"]);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    Out {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run(args: &[&str]) -> Out {
    cli(args, &[("CODERIPPER_CACHE", "off")])
}

fn manifest(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
}

/// A virtual workspace whose members each have one function with an unused parameter (so `unused-parameters` reports each).
fn workspace_of(members: &[(&str, &str)]) -> tempfile::TempDir {
    let list: Vec<String> = members
        .iter()
        .map(|(dir, _)| format!("\"{dir}\""))
        .collect();
    let root = format!(
        "[workspace]\nmembers = [{}]\nresolver = \"2\"\n",
        list.join(", ")
    );
    let mut files: Vec<(String, String)> = vec![("Cargo.toml".into(), root)];
    for (dir, package) in members {
        files.push((format!("{dir}/Cargo.toml"), manifest(package, "")));
        files.push((
            format!("{dir}/src/lib.rs"),
            format!(
                "pub fn from_{}(unused_{}: u32) {{}}\n",
                package.replace('-', "_"),
                package.replace('-', "_")
            ),
        ));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    common::git_repo_with(&refs)
}

/// The label that leads each finding line of `check` (`[Low/High] <label> — summary (check)`).
fn labels(stdout: &str, check: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| l.ends_with(&format!("({check})")))
        .filter_map(|l| l.split_once("] ").map(|(_, rest)| rest))
        .filter_map(|rest| {
            rest.split_once(" \u{2014} ")
                .map(|(label, _)| label.to_string())
        })
        .collect()
}

/// The finding lines without their leading label, for comparing two runs that label differently.
fn without_labels(stdout: &str, check: &str) -> BTreeSet<String> {
    stdout
        .lines()
        .filter(|l| l.ends_with(&format!("({check})")))
        .filter_map(|l| l.split_once(" \u{2014} ").map(|(_, rest)| rest.to_string()))
        .collect()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn every_member_is_checked_and_findings_name_their_member() {
    // directories differ from the package names, so a finding that fell back to the directory name would be visible
    let ws = workspace_of(&[("one", "alpha"), ("two", "beta")]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    let mut found = labels(&out.stdout, "unused-parameters");
    found.sort();
    assert_eq!(found, vec!["alpha", "beta"], "{}", out.stdout);
}

#[test]
fn findings_equal_those_of_running_each_member_by_hand() {
    // the property that also proves the session checkout changes no result
    let ws = workspace_of(&[("alpha", "alpha"), ("beta", "beta")]);
    let all = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(all.ok, "{}", all.stderr);
    let mut by_hand = BTreeSet::new();
    for member in ["alpha", "beta"] {
        let one = run(&[
            "check",
            "unused-parameters",
            "--project",
            path(&ws.path().join(member)),
        ]);
        assert!(one.ok, "{}", one.stderr);
        by_hand.extend(without_labels(&one.stdout, "unused-parameters"));
    }
    assert!(!by_hand.is_empty());
    assert_eq!(without_labels(&all.stdout, "unused-parameters"), by_hand);
}

#[test]
fn a_member_that_does_not_compile_does_not_stop_the_others_and_the_run_fails() {
    let ws = workspace_of(&[("alpha", "alpha"), ("beta", "beta"), ("gamma", "gamma")]);
    // break gamma, commit it
    std::fs::write(ws.path().join("gamma/src/lib.rs"), "pub fn broken( {\n").unwrap();
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "break gamma",
        ],
    ] {
        assert!(std::process::Command::new("git")
            .args(&args)
            .current_dir(ws.path())
            .status()
            .unwrap()
            .success());
    }
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(!out.ok, "a member error must fail the run");
    let mut found = labels(&out.stdout, "unused-parameters");
    found.sort();
    assert_eq!(
        found,
        vec!["alpha", "beta"],
        "{}\n{}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr.contains("gamma"),
        "the error must name the member: {}",
        out.stderr
    );
}

#[test]
fn repository_checks_run_once_not_per_member() {
    // two packages at different versions: version-consistency reports ONCE for the workspace
    let ws = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"alpha\", \"beta\"]\nresolver = \"2\"\n",
        ),
        ("alpha/Cargo.toml", &manifest("alpha", "")),
        ("alpha/src/lib.rs", "pub fn a() {}\n"),
        (
            "beta/Cargo.toml",
            &manifest("beta", "").replace("0.1.0", "0.2.0"),
        ),
        ("beta/src/lib.rs", "pub fn b() {}\n"),
    ]);
    let by_hand = run(&["check", "version-consistency", "--project", path(ws.path())]);
    let workspace = run(&[
        "check",
        "version-consistency",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    let count = |o: &Out| {
        o.stdout
            .lines()
            .filter(|l| l.ends_with("(version-consistency)"))
            .count()
    };
    assert!(
        count(&by_hand) >= 1,
        "precondition: the fixture must have a version finding: {}",
        by_hand.stdout
    );
    assert_eq!(count(&workspace), count(&by_hand), "{}", workspace.stdout);
}

#[test]
fn members_sharing_a_directory_name_are_told_apart() {
    let ws = workspace_of(&[("x/util", "x-util"), ("y/util", "y-util")]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    let mut found = labels(&out.stdout, "unused-parameters");
    found.sort();
    assert_eq!(found, vec!["x-util", "y-util"], "{}", out.stdout);
}

#[test]
fn a_member_directory_given_to_workspace_runs_the_whole_workspace() {
    let ws = workspace_of(&[("alpha", "alpha"), ("beta", "beta")]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(&ws.path().join("alpha")),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    let mut found = labels(&out.stdout, "unused-parameters");
    found.sort();
    assert_eq!(found, vec!["alpha", "beta"]);
}

#[test]
fn members_are_analysed_in_sorted_order_with_a_progress_line_each() {
    let ws = workspace_of(&[("c", "pc"), ("a", "pa"), ("b", "pb")]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    let progress: Vec<&str> = out
        .stderr
        .lines()
        .filter(|l| l.starts_with("coderipper: member "))
        .collect();
    assert_eq!(
        progress,
        vec![
            "coderipper: member pa (1/3)",
            "coderipper: member pb (2/3)",
            "coderipper: member pc (3/3)"
        ],
        "sorted by directory (a, b, c), which is not the declaration order"
    );
}

#[test]
fn a_dev_dependency_cycle_still_analyses_every_member() {
    let ws = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        ),
        (
            "a/Cargo.toml",
            &manifest("pa", "\n[dev-dependencies]\npb = { path = \"../b\" }\n"),
        ),
        ("a/src/lib.rs", "pub fn fa(unused_a: u32) {}\n"),
        (
            "b/Cargo.toml",
            &manifest("pb", "\n[dependencies]\npa = { path = \"../a\" }\n"),
        ),
        ("b/src/lib.rs", "pub fn fb(unused_b: u32) { pa::fa(0); }\n"),
    ]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    let mut found = labels(&out.stdout, "unused-parameters");
    found.sort();
    assert_eq!(found, vec!["pa", "pb"], "{}", out.stdout);
}

#[test]
fn a_root_package_workspace_reports_an_unknown_check_entry_once() {
    // the root is a package AND the workspace root: the repository-unit pass and the root member's pass both load
    // the same `.coderipper.toml`, and an entry naming no check must be reported once, not twice
    let ws = common::git_repo_with(&[
        ("Cargo.toml", &format!("{}\n[workspace]\nmembers = [\"sub\"]\n", manifest("rootpkg", ""))),
        ("src/lib.rs", "pub fn r() {}\n"),
        ("sub/Cargo.toml", &manifest("subpkg", "")),
        ("sub/src/lib.rs", "pub fn s() {}\n"),
        (
            ".coderipper.toml",
            "[[allow]]\ncheck = \"no-such-check\"\nfile = \"src/lib.rs\"\nsymbol = \"x\"\nreason = \"a typo\"\n",
        ),
    ]);
    let out = run(&["fast", "--workspace", "--project", path(ws.path())]);
    let unknown = out
        .stdout
        .lines()
        .filter(|l| l.contains("no-such-check"))
        .count();
    assert_eq!(unknown, 1, "{}\n{}", out.stdout, out.stderr);
}

#[test]
fn workspace_without_a_cargo_workspace_is_an_error_that_says_so() {
    let dir = common::git_repo_with(&[("README.md", "not a cargo project\n")]);
    let out = run(&[
        "check",
        "unused-parameters",
        "--workspace",
        "--project",
        path(dir.path()),
    ]);
    assert!(!out.ok);
    assert!(
        !out.stderr.contains("unexpected argument"),
        "the flag must exist; the failure must be about the project: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("cargo metadata failed"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_named_repository_check_runs_once_with_workspace() {
    let ws = workspace_of(&[("alpha", "alpha"), ("beta", "beta")]);
    let out = run(&[
        "check",
        "version-consistency",
        "--workspace",
        "--project",
        path(ws.path()),
    ]);
    assert!(out.ok, "{}\n{}", out.stdout, out.stderr);
    // no member of this fixture runs a package check, and no `member` progress line is needed for a repository check
    assert!(!out.stdout.contains("(unused-parameters)"));
}

// ---------------------------------------------------------------- the payoff

/// `(fresh, compiled)` from the CLI's `coderipper: cache ...` line.
fn stats(stderr: &str) -> (u64, u64) {
    let line = stderr
        .lines()
        .find(|l| l.starts_with("coderipper: cache "))
        .unwrap_or_else(|| panic!("no cache line in stderr:\n{stderr}"));
    let tail = line.split(" - ").nth(1).unwrap();
    let number = |s: &str| s.trim().parse::<u64>().unwrap();
    if let Some((fresh, rest)) = tail.split_once(" units fresh, ") {
        (
            number(fresh),
            number(rest.trim_end_matches(" compiled").trim()),
        )
    } else {
        (0, number(tail.trim_end_matches(" units compiled")))
    }
}

#[test]
fn the_siblings_are_compiled_once_per_session() {
    // a -> b -> c, all in the repository. Three separate `--project` runs each rebuild the siblings in their own new
    // checkout; one `--workspace` run builds each sibling once. Each side has its OWN fresh cache directory (a shared
    // one would let the first side warm the second), and one named check.
    let ws = common::git_repo_with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n",
        ),
        (
            "a/Cargo.toml",
            &manifest("sa", "\n[dependencies]\nsb = { path = \"../b\" }\n"),
        ),
        ("a/src/lib.rs", "pub fn fa(unused_a: u32) { sb::fb(0); }\n"),
        (
            "b/Cargo.toml",
            &manifest("sb", "\n[dependencies]\nsc = { path = \"../c\" }\n"),
        ),
        ("b/src/lib.rs", "pub fn fb(unused_b: u32) { sc::fc(0); }\n"),
        ("c/Cargo.toml", &manifest("sc", "")),
        ("c/src/lib.rs", "pub fn fc(unused_c: u32) {}\n"),
    ]);

    let mut by_hand_compiled = 0;
    for member in ["a", "b", "c"] {
        let cache = tempfile::tempdir().unwrap();
        let out = cli(
            &[
                "check",
                "unused-parameters",
                "--project",
                path(&ws.path().join(member)),
            ],
            &[("CODERIPPER_CACHE_DIR", path(cache.path()))],
        );
        assert!(out.ok, "{}", out.stderr);
        by_hand_compiled += stats(&out.stderr).1;
    }

    let cache = tempfile::tempdir().unwrap();
    let all = cli(
        &[
            "check",
            "unused-parameters",
            "--workspace",
            "--project",
            path(ws.path()),
        ],
        &[("CODERIPPER_CACHE_DIR", path(cache.path()))],
    );
    assert!(all.ok, "{}\n{}", all.stdout, all.stderr);
    let (fresh, compiled) = stats(&all.stderr);
    assert!(
        fresh >= 1,
        "the session must reuse a sibling it already built: {}",
        all.stderr
    );
    assert!(
        compiled < by_hand_compiled,
        "one --workspace run compiled {compiled} units, three --project runs {by_hand_compiled}: no sibling reuse"
    );
}
