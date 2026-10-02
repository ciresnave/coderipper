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
    assert_eq!(findings[0].subject.as_deref(), Some("dead_function"));
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

fn standalone_git_repo(cargo_toml: &str, main_rs: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("Cargo.toml"), cargo_toml).unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), main_rs).unwrap();
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

#[test]
fn a_crate_wide_allow_dead_code_makes_the_check_error_not_silently_report_clean() {
    // Review finding (Critical): a crate the rewrite can't actually detect dead code in (here,
    // because it blankets dead_code with #[allow] crate-wide) used to report zero findings AND
    // zero errors -- indistinguishable from "genuinely clean". The sentinel positive control (a
    // known-dead function injected every run) is the fix: if even the SENTINEL doesn't come back
    // as a hit, the run is untrustworthy and `run` must return Err, not Ok(vec![]).
    let repo = standalone_git_repo(
        "[package]\nname = \"blind\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        "#![allow(dead_code)]\nfn dead() {}\nfn main() {}\n",
    );
    let ctx = CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    };

    let result = ReachabilityCheck.run(&ctx);
    assert!(
        result.is_err(),
        "a run where the sentinel itself isn't detected must error, not silently report clean"
    );
}

#[test]
fn a_crate_that_fails_to_compile_after_the_rewrite_errors_instead_of_reporting_clean() {
    // Review finding (Critical), the CodeRipper-self-scan case: a package with a lib target AND a
    // bin/tests target that imports the lib by crate name fails to compile once the lib's pub
    // items are downgraded (E0603, "module is private") -- confirmed on CodeRipper's own repo
    // during review. `ReachabilityCheck::run` used to return `Ok(Vec::new())` for this, which
    // looked identical to "genuinely clean" from the caller's side. Reproduced here with a minimal
    // lib+bin package (not the full coderipper repo, to keep this test fast and self-contained).
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nname = \"libbin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/lib.rs"),
        "pub fn helper() -> i32 { 1 }\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/main.rs"),
        "fn main() { println!(\"{}\", libbin::helper()); }\n",
    )
    .unwrap();
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

    let ctx = CheckContext {
        project_root: tmp.path().to_path_buf(),
        portfolio_root: tmp.path().to_path_buf(),
    };

    let result = ReachabilityCheck.run(&ctx);
    assert!(
        result.is_err(),
        "a lib+bin crate that fails to compile after the rewrite must error, not report clean"
    );
}
