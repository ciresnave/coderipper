mod common;

use coderipper::check::{Check, CheckContext};
use coderipper::checks::reachability::ReachabilityCheck;
use common::{git_repo_with, MANIFEST};
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
    // Suppression is the host's job: the check itself returns the raw finding.
    assert_eq!(ReachabilityCheck.run(&ctx).unwrap().len(), 1);
    let result = coderipper::run_checks(&ctx, coderipper::check::Tier::Fast, Some("reachability"));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
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

fn subjects(findings: &[coderipper::finding::Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

fn run_reachability(repo: &tempfile::TempDir) -> anyhow::Result<Vec<coderipper::finding::Finding>> {
    ReachabilityCheck.run(&CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    })
}

const LIB_WITH_BIN_USERS: &str = "\
pub mod api {
    pub fn used_by_bin() -> i32 { 1 }
    pub fn helper_of_bin_only() -> i32 { 2 }
    pub fn used_by_lib() -> i32 { 3 }
    pub fn dead() -> i32 { 4 }
    pub struct Cfg;
    pub struct DeadTy;
}
pub use api::Cfg;
pub fn entry() -> i32 { api::used_by_lib() }
pub fn bin_root() -> i32 { api::helper_of_bin_only() }
";

const BIN_USING_THE_LIB: &str = "\
use fixture::Cfg;
fn main() {
    let _c = Cfg;
    println!(\"{} {}\", fixture::api::used_by_bin(), fixture::bin_root());
}
";

#[test]
fn a_lib_consumed_by_its_own_bin_is_analyzed_and_what_the_bin_uses_is_not_reported() {
    // The case the check used to refuse (E0603 after downgrading the lib's pub items): the lib is
    // built alone, and an item a bin names is rescued, together with everything it reaches.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB_WITH_BIN_USERS),
        ("src/main.rs", BIN_USING_THE_LIB),
    ]);
    let findings = run_reachability(&repo).unwrap();
    // used_by_bin, bin_root and Cfg are named by the bin; helper_of_bin_only is only reached from
    // bin_root (transitive rescue). Nothing in the lib or the bin reaches the other four.
    assert_eq!(
        subjects(&findings),
        vec!["DeadTy", "dead", "entry", "used_by_lib"]
    );
    assert!(findings.iter().all(|f| f.positive_control.is_some()));
    assert!(
        findings[0].detail.contains("not reported"),
        "the finding should say that candidates were set aside: {}",
        findings[0].detail
    );
}

#[test]
fn integration_tests_examples_benches_src_bin_and_bin_only_modules_all_rescue() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "pub fn from_test() {}\npub fn from_example() {}\npub fn from_bench() {}\npub fn from_bin() {}\npub fn from_bin_module() {}\npub fn nobody() {}\n",
        ),
        ("tests/t.rs", "#[test]\nfn t() { fixture::from_test(); }\n"),
        ("examples/e.rs", "fn main() { fixture::from_example(); }\n"),
        ("benches/b.rs", "fn main() { fixture::from_bench(); }\n"),
        ("src/bin/tool.rs", "fn main() { fixture::from_bin(); }\n"),
        ("src/main.rs", "mod util;\nfn main() { util::go(); }\n"),
        ("src/util.rs", "pub fn go() { fixture::from_bin_module(); }\n"),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["nobody"]);
}

#[test]
fn a_mention_inside_the_lib_itself_does_not_rescue() {
    // `src/inner.rs` is part of the lib, so its text is not a foreign mention: a dead item there
    // stays reported even though the bin mentions an unrelated name.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "mod inner;\npub use inner::kept_alive;\n"),
        (
            "src/inner.rs",
            "pub fn kept_alive() {}\npub fn dead_in_inner() {}\n",
        ),
        ("src/main.rs", "fn main() { fixture::kept_alive(); }\n"),
    ]);
    assert_eq!(
        subjects(&run_reachability(&repo).unwrap()),
        vec!["dead_in_inner"]
    );
}

#[test]
fn a_name_shared_with_something_the_bin_uses_is_rescued_that_is_the_accepted_over_rescue() {
    // `Tool::new` is dead in the lib; the bin calls a DIFFERENT `new`. By design the name wins:
    // a missed finding, never a wrongly reported live item.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub struct Tool;\nimpl Tool { pub fn new() -> Tool { Tool } }\npub struct Used;\nimpl Used { pub fn new() -> Used { Used } }\n"),
        ("src/main.rs", "fn main() { let _u = fixture::Used::new(); }\n"),
    ]);
    // Without the name match, the dead `Tool::new` would be the one finding.
    let found = subjects(&run_reachability(&repo).unwrap());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn deny_warnings_on_a_lib_does_not_stop_the_analysis() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#![deny(warnings)]\nuse std::collections::HashMap;\npub fn dead() {}\n",
        ),
        ("src/main.rs", "fn main() {}\n"),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["dead"]);
}

#[test]
fn a_crate_that_really_does_not_compile_still_errors() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f() { let x: i32 = \"no\"; }\n"),
        ("src/main.rs", "fn main() {}\n"),
    ]);
    assert!(run_reachability(&repo).is_err());
}

#[test]
fn a_path_from_the_bin_through_a_trait_impl_keeps_everything_it_reaches_alive() {
    // The shape a self-scan of CodeRipper itself exposed: the bin names `run_all`, which reaches
    // `registry`, whose trait impl's method calls `helper`. The trait impl is not a dead-code
    // candidate, so a rescue that only follows candidates loses the path and reports live code.
    let lib = "\
pub trait Check { fn go(&self) -> i32; }
pub struct A;
impl Check for A { fn go(&self) -> i32 { helper() } }
pub fn helper() -> i32 { 1 }
pub fn registry() -> Vec<Box<dyn Check>> { vec![Box::new(A)] }
pub fn run_all() -> i32 { registry().iter().map(|c| c.go()).sum() }
pub fn orphan() -> i32 { 2 }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        (
            "src/main.rs",
            "fn main() { println!(\"{}\", fixture::run_all()); }\n",
        ),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn a_constant_used_only_through_a_format_string_capture_is_alive() {
    let lib = "\
pub const GREETING: &str = \"hi\";
pub const NEVER: &str = \"unused\";
pub fn run_all() -> String { format!(\"{GREETING}, world\") }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        (
            "src/main.rs",
            "fn main() { println!(\"{}\", fixture::run_all()); }\n",
        ),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["NEVER"]);
}
