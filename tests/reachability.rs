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
    let ctx = CheckContext::new(repo.path().to_path_buf());

    let findings = ReachabilityCheck::new().run(&ctx).unwrap();

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

    let ctx = CheckContext::new(repo.path().to_path_buf());
    // Suppression is the host's job: the check itself returns the raw finding.
    assert_eq!(ReachabilityCheck::new().run(&ctx).unwrap().len(), 1);
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
    let ctx = CheckContext::new(repo.path().to_path_buf());

    let result = ReachabilityCheck::new().run(&ctx);
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
    ReachabilityCheck::new().run(&CheckContext::new(repo.path().to_path_buf()))
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

// ---- Review fixes: every fixture below is a LIVE item that a first version reported as dead. ----

fn bin_calls(call: &str) -> String {
    format!("fn main() {{ println!(\"{{}}\", {call}); }}\n")
}

#[test]
fn an_item_reached_only_through_an_internal_macro_is_alive() {
    let lib = "\
macro_rules! call_helper { () => { helper() } }
pub fn helper() -> i32 { 1 }
pub fn run() -> i32 { call_helper!() }
pub fn orphan() -> i32 { 2 }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", &bin_calls("fixture::run()")),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn an_item_reached_only_through_an_exported_macro_the_bin_invokes_is_alive() {
    let lib = "\
#[macro_export]
macro_rules! go { () => { $crate::exported_helper() } }
pub fn exported_helper() -> i32 { 1 }
pub fn orphan() -> i32 { 2 }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", &bin_calls("fixture::go!()")),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn an_item_reached_only_through_a_use_rename_is_alive() {
    let lib = "\
mod inner {
    pub fn real_name() -> i32 { 1 }
    pub fn orphan() -> i32 { 2 }
}
pub use inner::real_name as nice;
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", &bin_calls("fixture::nice()")),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn a_rename_inside_the_lib_itself_is_followed() {
    let lib = "\
mod inner { pub fn real() -> i32 { 1 } }
use inner::real as r;
pub fn run() -> i32 { r() }
pub fn orphan() -> i32 { 2 }
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", &bin_calls("fixture::run()")),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn bins_and_tests_with_a_custom_path_are_foreign_too() {
    let manifest = format!(
        "{MANIFEST}\n[[bin]]\nname = \"tool\"\npath = \"tools/tool.rs\"\n\n[[test]]\nname = \"it\"\npath = \"it/main.rs\"\n"
    );
    let repo = git_repo_with(&[
        ("Cargo.toml", &manifest),
        (
            "src/lib.rs",
            "pub fn used_by_custom_bin() {}\npub fn used_by_custom_test() {}\npub fn orphan() {}\n",
        ),
        (
            "tools/tool.rs",
            "mod helpers;\nfn main() { fixture::used_by_custom_bin(); helpers::go(); }\n",
        ),
        ("tools/helpers.rs", "pub fn go() {}\n"),
        (
            "it/main.rs",
            "#[test]\nfn t() { fixture::used_by_custom_test(); }\n",
        ),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn a_bound_in_an_inherent_impls_header_keeps_the_trait_alive() {
    let lib = "\
pub trait Helper { fn h(&self) -> u8; }
impl Helper for u16 { fn h(&self) -> u8 { 1 } }
pub struct S<T>(pub T);
impl<T: Helper> S<T> { pub fn get(&self) -> u8 { self.0.h() } }
pub fn orphan() {}
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", &bin_calls("fixture::S(3u16).get()")),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn a_trait_impl_is_live_because_its_type_is_not_because_of_a_keyword() {
    // `for` and `impl` appear in every trait impl header. A bin containing a `for` loop must not
    // make an impl for a type nobody reaches live: `NeverMade` and what its Drop calls stay reported.
    let lib = "\
pub struct NeverMade;
impl Drop for NeverMade { fn drop(&mut self) { only_from_drop(); } }
pub fn only_from_drop() {}
pub fn used() -> i32 { 1 }
";
    let bin = "fn main() { let mut n = 0; for i in 0..3 { n += i; } println!(\"{} {}\", n, fixture::used()); }\n";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", bin),
    ]);
    assert_eq!(
        subjects(&run_reachability(&repo).unwrap()),
        vec!["NeverMade", "only_from_drop"]
    );
}

#[test]
fn a_trait_impl_of_a_type_the_bin_names_is_live_even_though_nothing_names_its_methods() {
    // The case the previous test must not break: `Guard` is named by the bin, nobody names `drop`.
    let lib = "\
pub struct Guard;
impl Drop for Guard { fn drop(&mut self) { cleanup(); } }
pub fn cleanup() {}
pub fn orphan() {}
";
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/main.rs", "fn main() { let _g = fixture::Guard; }\n"),
    ]);
    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
}

#[test]
fn when_everything_is_set_aside_it_is_a_note_and_not_a_finding() {
    // 0.6.3: this was an Info finding, and Info fails a run by default; a count of what was not reported is bookkeeping
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "pub fn only_fn() -> i32 { 1 }
",
        ),
        ("src/main.rs", &bin_calls("fixture::only_fn()")),
    ]);
    let findings = run_reachability(&repo).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    // the host still says it, as a note
    let ctx = coderipper::check::CheckContext::new(repo.path());
    let result = coderipper::run_checks(&ctx, coderipper::check::Tier::Fast, Some("reachability"));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert!(
        result.notes.iter().any(|n| n.contains("set aside")),
        "{:?}",
        result.notes
    );
}

const PROC_MACRO_MANIFEST: &str = "\
[package]
name = \"fixture\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
proc-macro = true
";

const PROC_MACRO_LIB: &str = "\
mod helpers;

use proc_macro::TokenStream;

#[proc_macro_derive(Hello)]
pub fn derive_hello(_input: TokenStream) -> TokenStream {
    helpers::make()
}

#[proc_macro]
pub fn shout(input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_attribute]
pub fn tagged(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
";

#[test]
fn a_proc_macro_crate_runs_and_still_reports_its_dead_helper() {
    // Issue 54: the rewrite turned `pub fn derive_hello` into `pub(crate) fn`, which rustc rejects
    // ("functions tagged with `#[proc_macro_derive]` must be `pub`"), so every proc-macro crate was
    // "could not run" (vulkane_derive in the 2026-10-12 sweep). The three entry points stay `pub`;
    // an ordinary dead helper beside them must still be found.
    let repo = git_repo_with(&[
        ("Cargo.toml", PROC_MACRO_MANIFEST),
        ("src/lib.rs", PROC_MACRO_LIB),
        (
            "src/helpers.rs",
            "pub fn make() -> proc_macro::TokenStream { proc_macro::TokenStream::new() }\npub fn dead_helper() {}\n",
        ),
    ]);
    let findings = run_reachability(&repo).unwrap();
    assert_eq!(subjects(&findings), vec!["dead_helper"], "{findings:?}");
}

#[test]
fn a_proc_macro_crate_with_nothing_dead_is_clean() {
    // The clean twin of the test above: the same crate with the dead helper removed reports nothing,
    // so the finding there is the helper's and not an artifact of the macros.
    let repo = git_repo_with(&[
        ("Cargo.toml", PROC_MACRO_MANIFEST),
        ("src/lib.rs", PROC_MACRO_LIB),
        (
            "src/helpers.rs",
            "pub fn make() -> proc_macro::TokenStream { proc_macro::TokenStream::new() }\n",
        ),
    ]);
    let findings = run_reachability(&repo).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_proc_macro_written_inside_cfg_attr_runs_and_still_reports_its_dead_helper() {
    // The remainder of issue 54: 0.6.7 did not recognise the attribute inside `cfg_attr`, so a crate
    // that wrote its entry point that way was still "could not run" (exit 3).
    let lib = "\
mod helpers;

use proc_macro::TokenStream;

#[cfg_attr(not(test), proc_macro_derive(Hello))]
pub fn derive_hello(_input: TokenStream) -> TokenStream {
    helpers::make()
}
";
    let repo = git_repo_with(&[
        ("Cargo.toml", PROC_MACRO_MANIFEST),
        ("src/lib.rs", lib),
        (
            "src/helpers.rs",
            "pub fn make() -> proc_macro::TokenStream { proc_macro::TokenStream::new() }\npub fn dead_helper() {}\n",
        ),
    ]);
    let findings = run_reachability(&repo).unwrap();
    assert_eq!(subjects(&findings), vec!["dead_helper"], "{findings:?}");
}

/// A virtual workspace around the library `engine`:
/// - `app` depends on it and calls `used_by_app`;
/// - `mid` depends on it and glob re-exports it; `top` depends on `mid` (not on `engine`) and calls `used_through_mid`;
/// - `devuser` has `engine` as a dev-dependency and calls `used_by_devuser` from an integration test;
/// - `loner` depends on nothing, but defines a function with the same NAME as `used_by_loner_name`.
fn sibling_workspace() -> tempfile::TempDir {
    let package = |name: &str, deps: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{deps}")
    };
    let engine_dep = "\n[dependencies]\nengine = { path = \"../engine\" }\n";
    let files: Vec<(&str, String)> = vec![
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"engine\", \"app\", \"mid\", \"top\", \"devuser\", \"loner\"]\nresolver = \"2\"\n"
                .to_string(),
        ),
        ("engine/Cargo.toml", package("engine", "")),
        (
            "engine/src/lib.rs",
            "pub fn used_by_app() -> i32 { 1 }\n\
             pub fn used_through_mid() -> i32 { 2 }\n\
             pub fn used_by_devuser() -> i32 { 3 }\n\
             pub fn used_by_loner_name() -> i32 { 4 }\n\
             pub fn dead_everywhere() -> i32 { 5 }\n"
                .to_string(),
        ),
        ("app/Cargo.toml", package("app", engine_dep)),
        (
            "app/src/lib.rs",
            "pub fn run() -> i32 { engine::used_by_app() }\n".to_string(),
        ),
        ("mid/Cargo.toml", package("mid", engine_dep)),
        ("mid/src/lib.rs", "pub use engine::*;\n".to_string()),
        (
            "top/Cargo.toml",
            package("top", "\n[dependencies]\nmid = { path = \"../mid\" }\n"),
        ),
        (
            "top/src/main.rs",
            "fn main() { let _ = mid::used_through_mid(); }\n".to_string(),
        ),
        (
            "devuser/Cargo.toml",
            package(
                "devuser",
                "\n[dev-dependencies]\nengine = { path = \"../engine\" }\n",
            ),
        ),
        ("devuser/src/lib.rs", "pub fn nothing() {}\n".to_string()),
        (
            "devuser/tests/it.rs",
            "#[test]\nfn t() { assert_eq!(engine::used_by_devuser(), 3); }\n".to_string(),
        ),
        ("loner/Cargo.toml", package("loner", "")),
        (
            "loner/src/lib.rs",
            "pub fn used_by_loner_name() -> i32 { 9 }\n".to_string(),
        ),
    ];
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
    git_repo_with(&borrowed)
}

fn run_reachability_on_member(
    repo: &tempfile::TempDir,
    member: &str,
) -> anyhow::Result<Vec<coderipper::finding::Finding>> {
    ReachabilityCheck::new().run(&CheckContext::new(repo.path().join(member)))
}

#[test]
fn an_item_a_member_that_depends_on_the_crate_names_is_not_reported() {
    // Issue (2026-10-12 sweep): a member was judged alone, so a library's API used by its sibling crates
    // was reported dead (unisql: 8 of 8 sampled findings). Direct dependents, a re-exporting middle member
    // with a dependent of its own, and a dev-dependency's integration test all count.
    let repo = sibling_workspace();
    let findings = run_reachability_on_member(&repo, "engine").unwrap();
    assert_eq!(
        subjects(&findings),
        vec!["dead_everywhere", "used_by_loner_name"],
        "{findings:?}"
    );
}

#[test]
fn a_member_that_does_not_depend_on_the_crate_rescues_nothing() {
    // `loner` defines a function with the same name but cannot name engine's: it stays reported.
    let repo = sibling_workspace();
    let findings = run_reachability_on_member(&repo, "engine").unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.subject.as_deref() == Some("used_by_loner_name")),
        "{findings:?}"
    );
}

#[test]
fn the_finding_says_which_members_set_candidates_aside() {
    let repo = sibling_workspace();
    let findings = run_reachability_on_member(&repo, "engine").unwrap();
    let detail = &findings[0].detail;
    assert!(detail.contains("workspace member"), "{detail}");
    // all three rescued candidates were reached only through a member's files
    assert!(detail.contains(" 3 of them because "), "{detail}");
    assert!(
        detail.contains("(the members that do: app, devuser, mid, top)"),
        "{detail}"
    );
    assert!(!detail.contains("loner"), "{detail}");
}

#[test]
fn a_member_with_no_dependents_is_reported_exactly_as_before() {
    // `app` is depended on by nobody: its own API is judged as it always was.
    let repo = sibling_workspace();
    let findings = run_reachability_on_member(&repo, "app").unwrap();
    assert_eq!(subjects(&findings), vec!["run"], "{findings:?}");
    assert!(!findings[0].detail.contains("workspace member"));
}

fn summary_of<'a>(findings: &'a [coderipper::finding::Finding], subject: &str) -> &'a str {
    findings
        .iter()
        .find(|f| f.subject.as_deref() == Some(subject))
        .unwrap_or_else(|| panic!("no finding for {subject}: {findings:?}"))
        .summary
        .as_str()
}

#[test]
fn a_helper_only_a_dead_item_calls_says_so_instead_of_claiming_it_has_no_callers() {
    // Audit of 2026-10-12: `children_of` was reported as having "zero callers" although `eval_recipe` calls it;
    // `eval_recipe` is what has none. The claim must name the item the helper hangs from.
    let lib = "pub fn entry() -> i32 { helper() + other() }
fn helper() -> i32 { deeper() }
fn other() -> i32 { 2 }
fn deeper() -> i32 { 1 }
pub fn standalone() {}
";
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", lib)]);
    let findings = run_reachability(&repo).unwrap();
    assert_eq!(
        subjects(&findings),
        vec!["deeper", "entry", "helper", "other", "standalone"],
        "{findings:?}"
    );
    // the roots of what is dead keep the plain claim
    assert!(summary_of(&findings, "entry").contains("zero callers"));
    assert!(summary_of(&findings, "standalone").contains("zero callers"));
    // what hangs from them says where
    assert!(
        summary_of(&findings, "helper").contains("reachable only from `entry`"),
        "{findings:?}"
    );
    assert!(summary_of(&findings, "other").contains("reachable only from `entry`"));
    assert!(summary_of(&findings, "deeper").contains("reachable only from `helper`"));
    assert!(!summary_of(&findings, "helper").contains("zero callers"));
}

#[test]
fn a_dead_type_keeps_the_plain_claim_even_though_its_methods_mention_it() {
    // An inherent impl's header lends the type's name to its methods; the type is not "reachable only from" them.
    let lib = "pub struct Dead;
impl Dead {
    pub fn go(&self) {}
}
";
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", lib)]);
    let findings = run_reachability(&repo).unwrap();
    assert!(
        summary_of(&findings, "Dead").contains("zero callers"),
        "{findings:?}"
    );
}

#[test]
fn two_dead_functions_that_only_call_each_other_are_not_called_callerless() {
    // `a` calls `b` and `b` calls `a`: each has a caller, and neither is reachable from outside the pair.
    let lib = "pub fn a() { b(); }
fn b() { a(); }
";
    let repo = git_repo_with(&[("Cargo.toml", MANIFEST), ("src/lib.rs", lib)]);
    let findings = run_reachability(&repo).unwrap();
    assert_eq!(subjects(&findings), vec!["a", "b"], "{findings:?}");
    for subject in ["a", "b"] {
        let summary = summary_of(&findings, subject);
        assert!(summary.contains("through it or each other"), "{summary}");
        assert!(!summary.contains("zero callers"), "{summary}");
        assert!(!summary.contains("which has no callers"), "{summary}");
    }
}

/// `engine` plus `app`, which names engine by VERSION and is redirected to the workspace copy by the
/// root manifest's `patch_table` (cargo metadata --no-deps gives such a dependency no path).
fn patched_workspace(patch_table: &str) -> tempfile::TempDir {
    let files: Vec<(&str, String)> = vec![
        (
            "Cargo.toml",
            format!(
                "[workspace]
members = [\"engine\", \"app\"]
resolver = \"2\"

{patch_table}"
            ),
        ),
        (
            "engine/Cargo.toml",
            "[package]
name = \"engine\"
version = \"0.1.0\"
edition = \"2021\"
"
            .to_string(),
        ),
        (
            "engine/src/lib.rs",
            "pub fn used_by_app() -> i32 { 1 }
pub fn dead_everywhere() -> i32 { 2 }
"
            .to_string(),
        ),
        (
            "app/Cargo.toml",
            "[package]
name = \"app\"
version = \"0.1.0\"
edition = \"2021\"

[dependencies]
engine = \"0.1\"
"
            .to_string(),
        ),
        (
            "app/src/lib.rs",
            "pub fn run() -> i32 { engine::used_by_app() }
"
            .to_string(),
        ),
    ];
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
    git_repo_with(&borrowed)
}

#[test]
fn a_dependent_reaching_the_crate_through_a_patch_table_rescues() {
    // ROADMAP item 6(b): `[patch.crates-io]` redirects `engine = "0.1"` to the workspace copy.
    let repo = patched_workspace(
        "[patch.crates-io]
engine = { path = \"engine\" }
",
    );
    let findings = run_reachability_on_member(&repo, "engine").unwrap();
    assert_eq!(subjects(&findings), vec!["dead_everywhere"], "{findings:?}");
    assert!(
        findings[0].detail.contains("(the members that do: app)"),
        "{}",
        findings[0].detail
    );
}
