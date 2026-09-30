use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;

fn clean_fixture_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"clean-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    StdCommand::new("git").arg("init").arg("-q").current_dir(tmp.path()).status().unwrap();
    StdCommand::new("git").args(["add", "-A"]).current_dir(tmp.path()).status().unwrap();
    StdCommand::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    tmp
}

#[test]
fn fast_mode_runs_cleanly_against_a_real_clean_project() {
    // Review finding: the old version of this test ran `--project .` against the real coderipper
    // repo, a write into a shared checkout (`git worktree add`) that a CLI integration test must
    // not do -- fixed by using an isolated, throwaway fixture instead.
    let repo = clean_fixture_repo();
    Command::cargo_bin("coderipper")
        .unwrap()
        .arg("fast")
        .arg("--project")
        .arg(repo.path())
        .assert()
        .success();
}

#[test]
fn serve_mode_reports_not_implemented_rather_than_silently_doing_nothing() {
    Command::cargo_bin("coderipper")
        .unwrap()
        .arg("serve")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented"));
}

#[test]
fn unknown_check_id_produces_no_findings_without_running_any_check() {
    // Review finding: the old version of this test (and a sibling, since removed) ran against
    // `--project .` -- the real coderipper repo -- which is exactly the kind of write into a
    // shared checkout (`git worktree add`) a CLI integration test must not do. An unknown check id
    // is filtered out before any check's `run` is invoked, so a nonexistent path is safe here and
    // also proves the short-circuit: if `run` WERE called, it would fail loudly on this path.
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "does-not-exist", "--project", "/does/not/exist"])
        .assert()
        .failure(); // fails at path canonicalization, never reaches a check
}
