use coderipper::check::{Check, CheckContext};
use coderipper::checks::reachability::ReachabilityCheck;
use std::process::Command;

fn fixture_as_a_git_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let fixture_src = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/dead-pub-fn");
    for name in ["Cargo.toml", "src"] {
        let from = std::path::Path::new(fixture_src).join(name);
        let to = tmp.path().join(name);
        copy_recursive(&from, &to);
    }
    Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(tmp.path())
        .status()
        .unwrap();
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(tmp.path())
        .status()
        .unwrap();
    Command::new("git")
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

fn copy_recursive(from: &std::path::Path, to: &std::path::Path) {
    if from.is_dir() {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            copy_recursive(&entry.path(), &to.join(entry.file_name()));
        }
    } else {
        std::fs::copy(from, to).unwrap();
    }
}

#[test]
fn the_reachability_check_finds_exactly_the_dead_function() {
    let repo = fixture_as_a_git_repo();
    let ctx = CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    };

    let findings = ReachabilityCheck.run(&ctx).unwrap();

    assert_eq!(
        findings.len(),
        1,
        "exactly one dead pub item: dead_function"
    );
    assert!(findings[0].summary.contains("`dead_function`"));
    assert!(findings[0].positive_control.is_some());
    // caller/used_function must NOT appear as the IDENTIFIED symbol -- they're reachable via
    // main(). Match the backtick-quoted symbol name, not a bare substring: the summary template's
    // own prose ("has zero callers found") contains "caller" as a substring of "callers", which
    // would otherwise false-positive on every finding regardless of which symbol it's about.
    assert!(!findings
        .iter()
        .any(|f| f.summary.contains("`used_function`")));
    assert!(!findings.iter().any(|f| f.summary.contains("`caller`")));
}

#[test]
fn an_allowlisted_dead_symbol_is_suppressed() {
    let repo = fixture_as_a_git_repo();
    std::fs::write(
        repo.path().join(".coderipper.toml"),
        r#"
[[allow]]
check = "reachability"
file = "src/main.rs"
symbol = "dead_function"
reason = "test: confirm the allowlist suppresses a real finding"
"#,
    )
    .unwrap();

    let ctx = CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    };
    let findings = ReachabilityCheck.run(&ctx).unwrap();
    assert!(findings.is_empty());
}
