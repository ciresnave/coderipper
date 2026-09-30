use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn fast_mode_runs_cleanly_with_no_checks_registered() {
    Command::cargo_bin("coderipper")
        .unwrap()
        .arg("fast")
        .arg("--project")
        .arg(".")
        .assert()
        .success()
        .stdout(predicate::str::contains("no checks registered yet"));
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
fn unknown_check_id_produces_no_findings_not_a_crash() {
    Command::cargo_bin("coderipper")
        .unwrap()
        .args(["check", "does-not-exist", "--project", "."])
        .assert()
        .success();
}
