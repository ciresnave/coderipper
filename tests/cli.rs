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
    StdCommand::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(tmp.path())
        .status()
        .unwrap();
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    StdCommand::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ])
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
        .success()
        // Review finding: a genuinely clean run used to print "no checks registered yet", which is
        // false (a check IS registered) and indistinguishable from an unrelated failure mode.
        .stdout(predicate::str::contains("no checks registered yet").not());
}

#[test]
fn a_check_error_makes_the_cli_exit_non_zero() {
    // Review finding: `run_and_report` printed check errors to stderr but still returned `Ok(())`
    // from `main`, so the process exited 0 even when a check genuinely failed to run -- a CI
    // pipeline gating on exit code would see success. A crate with a syntax error fails to build
    // for reasons unrelated to dead_code, which the reachability check surfaces as a real error.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/main.rs"),
        "this is not valid rust {{{\n",
    )
    .unwrap();
    StdCommand::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(tmp.path())
        .status()
        .unwrap();
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    StdCommand::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ])
        .current_dir(tmp.path())
        .status()
        .unwrap();

    Command::cargo_bin("coderipper")
        .unwrap()
        .arg("fast")
        .arg("--project")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("check error"));
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

#[test]
fn the_unused_return_values_check_runs_by_id_and_reports_a_finding() {
    let repo = {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"discarder\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src/main.rs"),
            "fn f() -> i32 { 1 }\nfn main() { f(); }\n",
        )
        .unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        ] {
            StdCommand::new("git")
                .args(args)
                .current_dir(tmp.path())
                .status()
                .unwrap();
        }
        tmp
    };
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "unused-return-values", "--project"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("(unused-return-values)"))
        .stdout(predicate::str::contains("`f`"));
}

#[test]
fn a_stale_allowlist_entry_is_printed_as_an_informational_finding_and_does_not_fail_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"tidy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        tmp.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nsymbol = \"long_gone\"\nreason = \"was a real discard once\"\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    ] {
        StdCommand::new("git")
            .args(args)
            .current_dir(tmp.path())
            .status()
            .unwrap();
    }

    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "unused-return-values", "--project"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[Info/High]"))
        .stdout(predicate::str::contains("`long_gone`"))
        .stdout(predicate::str::contains("(allowlist)"));
}

#[test]
fn a_malformed_allowlist_fails_the_run_with_an_honest_message() {
    // Review finding: the error was reported as "1 check(s) failed to run" although no check failed.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"tidy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        tmp.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"x\"\n",
    )
    .unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    ] {
        StdCommand::new("git")
            .args(args)
            .current_dir(tmp.path())
            .status()
            .unwrap();
    }

    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "unused-return-values", "--project"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("allowlist"))
        .stderr(predicate::str::contains("check(s) failed to run").not());
}
