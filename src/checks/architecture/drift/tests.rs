// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn m(cycles: usize, surface: usize) -> Metrics {
    Metrics { cycles, surface }
}

#[test]
fn a_new_cycle_is_growth_and_an_unchanged_or_smaller_count_is_not() {
    let b = Budget::default();
    assert_eq!(
        compare(m(0, 50), m(1, 50), b),
        [Growth::Cycles { from: 0, to: 1 }]
    );
    assert!(compare(m(1, 50), m(1, 50), b).is_empty());
    assert!(compare(m(2, 50), m(1, 50), b).is_empty());
}

#[test]
fn surface_growth_needs_both_the_share_and_the_item_count() {
    let b = Budget::default();
    // 20 -> 28: 8 items (under 10), 40%: the item bound holds it back
    assert!(compare(m(0, 20), m(0, 28), b).is_empty());
    // 200 -> 215: 15 items but 7.5%: the share holds it back
    assert!(compare(m(0, 200), m(0, 215), b).is_empty());
    // 40 -> 60: 20 items, 50%: both
    assert_eq!(
        compare(m(0, 40), m(0, 60), b),
        [Growth::Surface { from: 40, to: 60 }]
    );
    // exactly at the share is not past it: 40 -> 50 is 25%
    assert!(compare(m(0, 40), m(0, 50), b).is_empty());
    // shrinking is never growth
    assert!(compare(m(0, 60), m(0, 40), b).is_empty());
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &std::path::Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

const MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// A repository tagged `v0.1.0` with two modules that do not refer to each other, then a commit that makes them a cycle and
/// adds public items.
fn repo(tag: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-q"]);
    write(p, "Cargo.toml", MANIFEST);
    write(p, "src/lib.rs", "pub mod a;\npub mod b;\n");
    write(p, "src/a.rs", "pub fn f() {}\n");
    write(p, "src/b.rs", "pub fn g() {}\n");
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "one"]);
    if tag {
        git(p, &["tag", "-a", "v0.1.0", "-m", "v0.1.0"]);
    }
    let many: String = (0..30)
        .map(|i| format!("pub fn extra{i}() {{}}\n"))
        .collect();
    write(
        p,
        "src/a.rs",
        &format!("pub fn f() {{ crate::b::g() }}\n{many}"),
    );
    write(p, "src/b.rs", "pub fn g() { crate::a::f() }\n");
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "two"]);
    dir
}

#[test]
fn a_cycle_and_a_surface_added_since_the_release_tag_are_reported() {
    let dir = repo(true);
    let (f, notes) = DriftCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    let subjects: Vec<_> = f.iter().filter_map(|x| x.subject.as_deref()).collect();
    assert_eq!(subjects, ["app:cycles", "app:surface"], "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
    assert_eq!(f[1].severity, Severity::Info);
    assert!(f[0].detail.contains("v0.1.0"), "{}", f[0].detail);
    // the host rejects a summary that reads as an absence claim without a positive control
    for x in &f {
        assert!(x.clone().validate().is_ok(), "{}", x.summary);
    }
    // positive control: the comparison ran
    assert!(
        notes.iter().any(|n| n.contains("compared 1 crate(s) of 1")),
        "{notes:?}"
    );
}

#[test]
fn a_repository_without_a_release_tag_is_not_run_and_a_base_setting_runs_it() {
    let dir = repo(false);
    let ctx = CheckContext::new(dir.path());
    let why = DriftCheck::new().not_configured(&ctx);
    assert!(
        why.is_some_and(|w| w.contains("tag a release")),
        "reported as not run"
    );
    write(
        dir.path(),
        ".coderipper.toml",
        "[arc009]\nbase = \"HEAD~1\"\n",
    );
    assert!(DriftCheck::new().not_configured(&ctx).is_none());
    let (f, _) = DriftCheck::new().run_noted(&ctx).unwrap();
    assert_eq!(f.len(), 2, "{f:?}");
}

#[test]
fn a_tag_on_head_itself_is_not_the_base_and_the_one_before_it_is() {
    let dir = repo(true);
    let p = dir.path();
    assert_eq!(base_of(p).unwrap().unwrap().name, "v0.1.0");
    git(p, &["tag", "-a", "v0.2.0", "-m", "v0.2.0"]);
    assert_eq!(
        base_of(p).unwrap().unwrap().name,
        "v0.1.0",
        "HEAD is tagged v0.2.0, so the previous release is the base"
    );
}

#[test]
fn an_unknown_base_is_an_error_naming_it() {
    let dir = repo(true);
    write(
        dir.path(),
        ".coderipper.toml",
        "[arc009]\nbase = \"v9.9.9\"\n",
    );
    let e = DriftCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap_err();
    assert!(e.to_string().contains("v9.9.9"), "{e}");
}
