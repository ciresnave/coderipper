#![cfg(feature = "cli")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Configuration layers (ROADMAP item 5, `docs/superpowers/plans/2026-10-13-configuration-layers.md`): a wide
//! `.coderipper.toml` in the user's home (or the file `--config` names), the repository's own file restricted to an allowlist
//! of keys, the environment, then flags. Each test runs the real binary with `CODERIPPER_CONFIG` naming the test's "home" file
//! (or `CODERIPPER_NO_CONFIG` when it has none), so a developer's own file never takes part. (`HOME` is left alone: the build
//! tools the checks run need it.) Where the search finds the files is a unit test of `config::search_paths`.

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const MEDIUM: &str = "fn f() -> i32 { 1 }\nfn main() { f(); }\n";

fn medium_project(extra: &[(&str, &str)]) -> tempfile::TempDir {
    let mut files = vec![("Cargo.toml", MANIFEST), ("src/main.rs", MEDIUM)];
    files.extend_from_slice(extra);
    common::git_repo_with(&files)
}

fn clean_project() -> tempfile::TempDir {
    common::git_repo_with(&[("Cargo.toml", MANIFEST), ("src/main.rs", "fn main() {}\n")])
}

/// The CLI with the cache OFF and the wide configuration being `home`'s `.coderipper.toml` when the test wrote one, and none
/// otherwise.
fn coderipper(home: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off")
        .env_remove("CODERIPPER_CONFIG")
        .env_remove("CODERIPPER_NO_CONFIG")
        .env_remove("CODERIPPER_TOOLS_DIR");
    let file = home.join(".coderipper.toml");
    if file.exists() {
        cmd.env("CODERIPPER_CONFIG", file);
    } else {
        cmd.env("CODERIPPER_NO_CONFIG", "1");
    }
    cmd
}

fn home_with(toml: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".coderipper.toml"), toml).unwrap();
    home
}

fn check(home: &Path, project: &Path, extra: &[&str]) -> assert_cmd::assert::Assert {
    coderipper(home)
        .args(["check", "unused-return-values"])
        .args(extra)
        .arg("--project")
        .arg(project)
        .assert()
}

#[test]
fn a_home_file_sets_a_default_that_a_flag_overrides() {
    let home = home_with("deny = \"none\"\n");
    let repo = medium_project(&[]);
    check(home.path(), repo.path(), &[])
        .code(0)
        .stderr(predicate::str::contains(".coderipper.toml"));
    check(home.path(), repo.path(), &["--deny", "info"]).code(1);
}

#[test]
fn no_config_reads_none_of_the_wide_files() {
    let home = home_with("deny = \"none\"\n");
    let repo = medium_project(&[]);
    check(home.path(), repo.path(), &["--no-config"]).code(1);
    coderipper(home.path())
        .env("CODERIPPER_NO_CONFIG", "1")
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .assert()
        .code(1);
}

#[test]
fn config_reads_exactly_that_file_in_place_of_the_home_file() {
    let home = home_with("deny = \"none\"\n");
    let other = tempfile::tempdir().unwrap();
    let file = other.path().join("strict.toml");
    std::fs::write(&file, "deny = \"low\"\n").unwrap();
    let repo = medium_project(&[]);
    check(
        home.path(),
        repo.path(),
        &["--config", file.to_str().unwrap()],
    )
    .code(1);
    // a file named by --config that is not there is an error, not an absence
    check(
        home.path(),
        repo.path(),
        &[
            "--config",
            other.path().join("missing.toml").to_str().unwrap(),
        ],
    )
    .code(2)
    .stderr(predicate::str::contains("missing.toml"));
}

#[test]
fn config_and_no_config_together_are_a_usage_error() {
    let home = tempfile::tempdir().unwrap();
    let repo = clean_project();
    check(
        home.path(),
        repo.path(),
        &["--config", "x.toml", "--no-config"],
    )
    .code(2)
    .stderr(predicate::str::contains("cannot be used together"));
}

#[test]
fn a_repository_file_cannot_set_what_is_not_on_its_allowlist() {
    let home = tempfile::tempdir().unwrap();
    // `deny = "none"` would let this repository pass its own gate; `install-tools` would grant itself downloads; the
    // directories would put the tool cache and build cache inside the repository.
    let repo = medium_project(&[(
        ".coderipper.toml",
        "deny = \"none\"\nredact = false\ninstall-tools = true\n\n[tools]\ndir = \"evil-tools\"\n\n[cache]\ndir = \"evil-cache\"\n\n[limits]\nmax_runtime_minutes = 100000\n",
    )]);
    check(home.path(), repo.path(), &[])
        .code(1)
        .stderr(predicate::str::contains("deny"))
        .stderr(predicate::str::contains("install-tools"))
        .stderr(predicate::str::contains("tools.dir"))
        .stderr(predicate::str::contains("cache.dir"))
        .stderr(predicate::str::contains("limits.max_runtime_minutes"));
    assert!(!repo.path().join("evil-tools").exists());
    assert!(!repo.path().join("evil-cache").exists());
}

#[test]
fn a_repository_file_cannot_grant_itself_tool_installs() {
    let home = tempfile::tempdir().unwrap();
    let repo = medium_project(&[(".coderipper.toml", "install-tools = true\n")]);
    coderipper(home.path())
        .args([
            "fast",
            "--profile",
            "extended",
            "--deny",
            "none",
            "--project",
        ])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("install-tools"))
        // the delegated rules still say they were not run for want of the user's consent
        .stderr(predicate::str::contains("not run"));
}

#[test]
fn an_unknown_key_in_the_repository_file_is_an_error() {
    let home = tempfile::tempdir().unwrap();
    let repo = medium_project(&[(".coderipper.toml", "dney = \"none\"\n")]);
    check(home.path(), repo.path(), &["--deny", "none"])
        .code(3)
        .stderr(predicate::str::contains("dney"));
}

#[test]
fn a_repository_file_may_set_the_profile_and_a_flag_wins() {
    let home = tempfile::tempdir().unwrap();
    let repo = clean_project();
    std::fs::write(
        repo.path().join(".coderipper.toml"),
        "profile = \"extended\"\n",
    )
    .unwrap();
    let run = |extra: &[&str]| {
        coderipper(home.path())
            .args(["fast", "--deny", "none", "--project"])
            .arg(repo.path())
            .args(extra)
            .assert()
    };
    // the extended profile adds the coverage report
    run(&[]).stdout(predicate::str::contains("Not yet implemented"));
    run(&["--profile", "classic"]).stdout(predicate::str::contains("Not yet implemented").not());
}

#[test]
fn a_home_file_may_waive_and_the_waiver_says_where_it_came_from() {
    let home = home_with(
        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nreason = \"discarded on purpose in this fixture\"\n",
    );
    let repo = medium_project(&[]);
    let file = home.path().join(".coderipper.toml");
    check(home.path(), repo.path(), &[])
        .code(0)
        .stdout(predicate::str::contains("waived by"))
        .stdout(predicate::str::contains(file.to_str().unwrap()))
        .stdout(predicate::str::contains("discarded on purpose"))
        // one warning per run, naming the file and the count, and the run still goes on
        .stderr(predicate::str::contains("1 [[allow]] entr"))
        .stderr(predicate::str::contains("belong in each repository"));
    // --no-config: the same repository fails, which is how a person checks the reproducibility cost
    check(home.path(), repo.path(), &["--no-config"]).code(1);
}

#[test]
fn a_wide_waiver_that_matches_nothing_here_is_not_a_stale_entry() {
    let home = home_with(
        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/elsewhere.rs\"\nreason = \"belongs to another repository\"\n",
    );
    let repo = clean_project();
    coderipper(home.path())
        .args(["fast", "--profile", "classic", "--project"])
        .arg(repo.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("allowlist").not());
}

#[test]
fn a_waiver_in_the_repository_file_is_still_stale_when_it_matches_nothing() {
    let home = tempfile::tempdir().unwrap();
    let repo = clean_project();
    std::fs::write(
        repo.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nreason = \"no longer needed\"\n",
    )
    .unwrap();
    coderipper(home.path())
        .args(["fast", "--project"])
        .arg(repo.path())
        .assert()
        .code(1)
        .stdout(predicate::str::contains("allowlist"));
}

#[test]
fn a_broken_wide_file_is_a_usage_error_naming_the_file() {
    let home = home_with("deny = \"sometimes\"\n");
    let repo = clean_project();
    check(home.path(), repo.path(), &[])
        .code(2)
        .stderr(predicate::str::contains(".coderipper.toml"))
        .stderr(predicate::str::contains("deny"));
}

#[test]
fn the_cache_section_of_a_home_file_is_a_default_under_the_environment() {
    // `[cache] enabled = false` turns the cache off ...
    let home = home_with("[cache]\nenabled = false\n");
    coderipper(home.path())
        .env_remove("CODERIPPER_CACHE")
        .args(["cache", "status"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("the build cache is off"));
    // ... and the environment wins over the file
    let cache = tempfile::tempdir().unwrap();
    std::fs::write(cache.path().join(".coderipper-cache"), "").unwrap();
    coderipper(home.path())
        .env("CODERIPPER_CACHE", "on")
        .env("CODERIPPER_CACHE_DIR", cache.path())
        .args(["cache", "status"])
        .assert()
        .code(0);
}

#[test]
fn a_limit_is_reported_and_only_ever_lowered() {
    let home = home_with(
        "[limits]
max_runtime_minutes = 30
",
    );
    let repo = clean_project();
    check(home.path(), repo.path(), &[])
        .code(0)
        .stderr(predicate::str::contains("max_runtime_minutes = 30"));
    // the environment may lower the ceiling ...
    coderipper(home.path())
        .env("CODERIPPER_MAX_RUNTIME_MINUTES", "10")
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("max_runtime_minutes = 10"));
    // ... and never raise it
    coderipper(home.path())
        .env("CODERIPPER_MAX_RUNTIME_MINUTES", "120")
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .assert()
        .stderr(predicate::str::contains("max_runtime_minutes = 30"));
}

#[test]
fn a_repository_that_is_a_wide_file_is_not_told_its_own_keys_are_skipped() {
    // the repository's file is also the wide file (a repository at the home directory): its keys apply as wide keys
    let repo = clean_project();
    let file = repo.path().join(".coderipper.toml");
    std::fs::write(
        &file,
        "deny = \"none\"
",
    )
    .unwrap();
    coderipper(repo.path())
        .args(["fast", "--project"])
        .arg(repo.path())
        .assert()
        .code(0)
        .stderr(predicate::str::contains("skipped").not());
}

#[test]
fn a_home_file_may_declare_public_api_and_says_where_it_came_from() {
    let home = home_with(
        "[reachability]\npublic_api = [{ file = \"src/lib.rs\", reason = \"consumed by downstream crates\" }]\n",
    );
    let repo = common::git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn api() {}\n"),
    ]);
    let file = home.path().join(".coderipper.toml");
    coderipper(home.path())
        .args(["check", "reachability", "--project"])
        .arg(repo.path())
        .assert()
        .code(0)
        .stdout(predicate::str::contains("[declared-public/"))
        .stdout(predicate::str::contains("declared by"))
        .stdout(predicate::str::contains(file.to_str().unwrap()))
        .stderr(predicate::str::contains("1 [reachability] public_api entr"));
}
