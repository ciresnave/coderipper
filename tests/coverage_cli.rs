#![cfg(feature = "cli")]
//! The coverage report and the profiles from the command line (multi-language design sections 4.4, 6.4, 10.2).
//!
//! The owner's ruling (2026-10-08): a known coverage gap NEVER fails a run. It may be mentioned as "not yet implemented", worded as
//! CodeRipper's incomplete implementation and not as a finding about the checked code. The default `classic` profile prints
//! exactly what it printed before, apart from one stderr line for a language that has no module.

#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use assert_cmd::Command;
use predicates::prelude::*;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// What the language-neutral rules ask of a project with a binary: a committed lockfile and a default owner.
const LOCK: &str = "version = 4\n\n[[package]]\nname = \"fixture\"\nversion = \"0.1.0\"\n";
const OWNERS: &str = "* @example-owner\n";

/// A Rust project with nothing for any check to say, under either profile.
fn clean_project() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("Cargo.lock", LOCK),
        (".github/CODEOWNERS", OWNERS),
        ("src/main.rs", "fn main() {}\n"),
    ])
}

/// A clean Rust project that also contains a TypeScript package no module checks.
fn polyglot_project() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("Cargo.lock", LOCK),
        (".github/CODEOWNERS", OWNERS),
        ("src/main.rs", "fn main() {}\n"),
        ("web/package.json", "{}\n"),
        ("web/app.ts", "export {};\n"),
        ("web/util.ts", "export {};\n"),
    ])
}

/// A project with one Medium finding from `unused-return-values` (`f`'s result is always discarded).
fn project_with_a_medium_finding() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("Cargo.lock", LOCK),
        (".github/CODEOWNERS", OWNERS),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
    ])
}

fn fast(project: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.arg("fast").arg("--project").arg(project.path());
    cmd
}

#[test]
fn the_classic_profile_prints_nothing_about_coverage() {
    fast(&clean_project())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("coverage").not())
        .stderr(predicate::str::contains("not checked").not());
}

#[test]
fn classic_says_one_stderr_line_for_a_language_with_no_module_and_leaves_stdout_alone() {
    fast(&polyglot_project())
        .assert()
        .code(0)
        .stdout("coderipper: no issues found\n")
        .stderr(predicate::str::contains(
            "coderipper: typescript: 2 source files found, not checked (classic profile; see --profile extended)",
        ));
}

#[test]
fn classic_json_is_unchanged_by_an_unchecked_language() {
    let out = fast(&polyglot_project())
        .args(["--message-format", "json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 1, "only the summary line: {text}");
    assert!(text.contains("coderipper-summary"));
}

#[test]
fn extended_prints_the_coverage_report_and_no_stderr_line() {
    fast(&polyglot_project())
        .args(["--profile", "extended"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("coverage: rust"))
        .stdout(predicate::str::contains("coverage: typescript"))
        .stdout(predicate::str::contains("no module for typescript"))
        .stderr(predicate::str::contains("not checked").not());
}

#[test]
fn a_run_with_only_gaps_exits_success_and_prints_the_not_yet_implemented_section() {
    // The owner's ruling: a coverage gap never fails a run, not even under the strictest --deny.
    fast(&clean_project())
        .args(["--profile", "extended", "--deny", "info"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("Not yet implemented"))
        .stdout(predicate::str::contains("says nothing about your code"));
}

#[test]
fn the_gap_section_is_worded_as_coderippers_incompleteness_never_as_a_finding() {
    let out = fast(&clean_project())
        .args(["--profile", "extended"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap().to_lowercase();
    for banned in ["failed", "violation", "defect", "[high", "[medium", "[low"] {
        assert!(!text.contains(banned), "{banned} in {text}");
    }
}

#[test]
fn a_real_finding_still_fails_the_run_under_deny_gaps_or_no_gaps() {
    fast(&project_with_a_medium_finding())
        .args(["--profile", "extended", "--deny", "medium"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("unused-return-values"))
        .stdout(predicate::str::contains("coverage: rust"));
}

#[test]
fn the_coverage_flag_alone_adds_the_report_without_the_extended_profile() {
    fast(&clean_project())
        .arg("--coverage")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("coverage: rust"))
        .stdout(predicate::str::contains("use --coverage=full to list"));
}

#[test]
fn coverage_full_lists_the_gaps_by_rule_id() {
    fast(&clean_project())
        .arg("--coverage=full")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("MOD-002"))
        .stdout(predicate::str::contains("ci-protection-presence"));
}

#[test]
fn the_report_counts_this_runs_outcomes() {
    fast(&project_with_a_medium_finding())
        .arg("--coverage")
        .assert()
        .code(0)
        .stdout(predicate::str::is_match(r"this run: \d+ clean, 1 with findings").unwrap());
}

#[test]
fn json_coverage_lines_come_before_the_summary_and_follow_the_documented_shape() {
    let out = fast(&polyglot_project())
        .args(["--profile", "extended", "--message-format", "json"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let reasons: Vec<&str> = lines
        .iter()
        .map(|l| l["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        [
            "coderipper-coverage",
            "coderipper-coverage",
            "coderipper-summary"
        ],
        "{text}"
    );
    let rust = &lines[0];
    assert_eq!(rust["language"], "rust");
    // the four proven Rust checks and the five proven language-neutral rules
    assert_eq!(rust["covered"], 9);
    assert_eq!(rust["claimed_unproven"], 1);
    assert!(rust["run"]["clean"].as_u64().unwrap() >= 1);
    let ts = &lines[1];
    assert_eq!(ts["language"], "typescript");
    // no TypeScript module, but the five language-neutral rules apply to it
    assert_eq!(ts["covered"], 5);
    assert_eq!(
        ts["note"],
        "no module for typescript, only the language-neutral rules"
    );
    assert_eq!(ts["source_files"], 2);
    // The exit code in the summary is untouched by the gaps.
    assert_eq!(lines[2]["exit_code"], 0);
}

#[test]
fn an_unknown_profile_is_a_usage_error() {
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["fast", "--profile", "nonsense"])
        .assert()
        .code(2);
}

/// A clean Rust project that also contains a Python package: SUP-001 reads Cargo and npm files only.
fn python_project() -> tempfile::TempDir {
    common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("Cargo.lock", LOCK),
        (".github/CODEOWNERS", OWNERS),
        ("src/main.rs", "fn main() {}\n"),
        ("py/pyproject.toml", "[project]\nname = \"p\"\n"),
        ("py/app.py", "print(1)\n"),
    ])
}

#[test]
fn a_language_the_lockfile_rule_does_not_read_shows_it_as_a_gap_but_typescript_does_not() {
    let out = fast(&python_project())
        .args([
            "--profile",
            "extended",
            "--coverage=full",
            "--message-format",
            "json",
        ])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let py = lines
        .iter()
        .find(|l| l["language"] == "python")
        .unwrap_or_else(|| panic!("{text}"));
    // the four rules that read any file kind are covered; SUP-001 reads Cargo and npm files only
    assert_eq!(py["covered"], 4, "{py}");
    let gaps: Vec<&str> = py["gaps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    assert!(gaps.contains(&"SUP-001"), "{py}");
    for covered in ["SUP-011", "DOC-005", "DOC-009", "WSP-001"] {
        assert!(!gaps.contains(&covered), "{covered} in {gaps:?}");
    }
    // positive control: for the Rust language the same rule is covered, so it is the language that makes the difference
    let rust = lines.iter().find(|l| l["language"] == "rust").unwrap();
    assert_eq!(rust["covered"], 9, "{rust}");
}

#[test]
fn coverage_full_names_the_lockfile_rule_among_pythons_gaps() {
    fast(&python_project())
        .args(["--profile", "extended", "--coverage=full"])
        .assert()
        .code(0)
        .stdout(predicate::str::is_match(r"python: .*SUP-001").unwrap());
}
