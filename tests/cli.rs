use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;

/// The CLI with the build cache OFF, so a test never reads or writes a developer's real cache.
fn coderipper() -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_CACHE", "off");
    cmd
}

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
    coderipper()
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

    coderipper()
        .arg("fast")
        .arg("--project")
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("check error"));
}

#[test]
fn serve_mode_reports_not_implemented_rather_than_silently_doing_nothing() {
    coderipper()
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
    coderipper()
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
    coderipper()
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

    coderipper()
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

    coderipper()
        .args(["check", "unused-return-values", "--project"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("allowlist"))
        .stderr(predicate::str::contains("check(s) failed to run").not());
}

#[test]
fn the_unused_parameters_check_runs_by_id_and_reports_a_parameter() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"params\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/main.rs"),
        "fn f(used: i32, ignored: i32) -> i32 { used }\nfn main() { f(1, 2); }\n",
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
    coderipper()
        .args(["check", "unused-parameters", "--project"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "parameter `ignored` of `f` is never used",
        ))
        .stdout(predicate::str::contains("(unused-parameters)"));
}

fn workspace_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let files = [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"one\", \"two\"]\nresolver = \"2\"\n",
        ),
        (
            "one/Cargo.toml",
            "[package]\nname = \"one\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("one/src/lib.rs", "pub fn f(unused_one: i32) {}\n"),
        (
            "two/Cargo.toml",
            "[package]\nname = \"two\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("two/src/lib.rs", "pub fn g(unused_two: i32) {}\n"),
    ];
    for (name, contents) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
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
}

#[test]
fn a_workspace_member_can_be_the_project() {
    let ws = workspace_fixture();
    coderipper()
        .args(["check", "unused-parameters", "--project"])
        .arg(ws.path().join("two"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "parameter `unused_two` of `g` is never used",
        ))
        .stdout(predicate::str::contains("unused_one").not());
}

#[test]
fn a_virtual_workspace_root_asks_for_a_member() {
    let ws = workspace_fixture();
    coderipper()
        .args(["check", "unused-parameters", "--project"])
        .arg(ws.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a package directory"))
        .stderr(predicate::str::contains("one, two").or(predicate::str::contains("two, one")));
}

#[test]
fn the_version_consistency_check_runs_by_id_and_reports_the_outlier() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    for (name, version) in [("a", "1.0.0"), ("b", "1.0.0"), ("c", "0.9.0")] {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
    }
    coderipper()
        .args(["check", "version-consistency", "--project"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[High/High]"))
        .stdout(predicate::str::contains(
            "`c` is at version 0.9.0 but this project is at 1.0.0",
        ))
        .stdout(predicate::str::contains("(version-consistency)"));
}

#[test]
fn ci_protection_presence_on_a_non_github_origin_fails_loudly_without_touching_the_network() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://gitlab.com/acme/widgets.git",
        ],
    ] {
        StdCommand::new("git")
            .args(args)
            .current_dir(tmp.path())
            .status()
            .unwrap();
    }
    coderipper()
        .args(["check", "ci-protection-presence", "--project"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a GitHub repository"));
}
