// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::checks::rust_sources::SourceFile;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn facts(files: &[SourceFile]) -> Facts {
    modgraph::read(files).unwrap().unwrap()
}

fn cycles(files: &[SourceFile]) -> (Vec<Finding>, BTreeMap<(String, String), Site>) {
    let f = facts(files);
    let (edges, nodes) = module_edges(&f);
    let sets = sccs(&nodes, &edges.keys().cloned().collect());
    (judge(&f, &edges, &sets, "p"), edges)
}

fn edge_names(e: &BTreeMap<(String, String), Site>) -> Vec<String> {
    e.keys().map(|(a, b)| format!("{a}->{b}")).collect()
}

#[test]
fn modules_in_different_branches_that_refer_to_each_other_are_a_low_cycle() {
    let (f, edges) = cycles(&[
        file("src/lib.rs", "mod a;\nmod b;\n"),
        file("src/a.rs", "pub fn f() { crate::b::g() }\n"),
        file("src/b.rs", "pub fn g() { crate::a::f() }\n"),
    ]);
    assert_eq!(edge_names(&edges), ["a->b", "b->a"]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Low);
    assert_eq!(f[0].subject.as_deref(), Some("a,b"));
    assert!(
        f[0].summary.contains("across top-level modules"),
        "{}",
        f[0].summary
    );
    assert!(f[0].detail.contains("(src/a.rs:1)"), "{}", f[0].detail);
}

#[test]
fn a_one_way_chain_is_no_cycle_and_the_edges_were_read() {
    let (f, edges) = cycles(&[
        file("src/lib.rs", "mod a;\nmod b;\n"),
        file("src/a.rs", "pub fn f() { crate::b::g() }\n"),
        file("src/b.rs", "pub fn g() {}\n"),
    ]);
    assert!(f.is_empty(), "{f:?}");
    // positive control: the edge a -> b exists
    assert_eq!(edge_names(&edges), ["a->b"]);
}

#[test]
fn a_child_naming_its_parent_through_super_or_self_is_not_a_dependency() {
    let (f, edges) = cycles(&[
        file("src/lib.rs", "mod a;\n"),
        file("src/a.rs", "mod c;\npub fn x() {}\npub fn y() { c::z() }\n"),
        file("src/a/c.rs", "pub fn z() { super::x() }\n"),
    ]);
    // the parent uses the child (a -> a::c); the child's `super::x` is containment
    assert_eq!(edge_names(&edges), ["a->a::c"]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_descendant_that_reaches_back_by_an_absolute_path_makes_an_info_cycle_inside_one_top_level_module(
) {
    let (f, _) = cycles(&[
        file("src/lib.rs", "mod a;\n"),
        file("src/a.rs", "mod c;\npub fn x() {}\npub fn y() { c::z() }\n"),
        file("src/a/c.rs", "pub fn z() { crate::a::x() }\n"),
    ]);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].subject.as_deref(), Some("a,a::c"));
    assert!(
        f[0].summary
            .contains("inside the top-level module `crate::a`"),
        "{}",
        f[0].summary
    );
}

#[test]
fn the_crate_root_takes_no_part() {
    // the root uses a, and a uses an item the root defines: the root is the façade, not a module a depends on
    let (f, edges) = cycles(&[
        file(
            "src/lib.rs",
            "mod a;\npub struct Thing;\npub fn run() { a::f() }\n",
        ),
        file("src/a.rs", "pub fn f() { let _ = crate::Thing; }\n"),
    ]);
    assert!(edges.is_empty(), "{edges:?}");
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn the_use_that_builds_a_re_export_is_not_an_edge() {
    let (f, edges) = cycles(&[
        file("src/lib.rs", "mod a;\n"),
        file("src/a.rs", "mod c;\npub use c::X;\npub fn y() {}\n"),
        file("src/a/c.rs", "pub struct X;\nfn z() { crate::a::y() }\n"),
    ]);
    // c -> a by an absolute path counts; a -> c exists only through `pub use c::X`, the interface
    assert_eq!(edge_names(&edges), ["a::c->a"]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn test_items_are_not_read() {
    let (f, _) = cycles(&[
        file("src/lib.rs", "mod a;\nmod b;\n"),
        file("src/a.rs", "pub fn f() { crate::b::g() }\n"),
        file(
            "src/b.rs",
            "pub fn g() {}\n#[cfg(test)]\nmod t { fn h() { crate::a::f() } }\n",
        ),
    ]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_three_module_loop_names_every_single_edge_that_dissolves_it() {
    let (f, _) = cycles(&[
        file("src/lib.rs", "mod a;\nmod b;\nmod c;\n"),
        file("src/a.rs", "pub fn f() { crate::b::g() }\n"),
        file("src/b.rs", "pub fn g() { crate::c::h() }\n"),
        file("src/c.rs", "pub fn h() { crate::a::f() }\n"),
    ]);
    assert_eq!(f.len(), 1);
    assert!(f[0].summary.starts_with("3 modules"), "{}", f[0].summary);
    assert!(
        f[0].detail
            .contains("Removing any one of these edges dissolves it"),
        "{}",
        f[0].detail
    );
}

fn repo(b_src: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let w = |p: &str, t: &str| {
        let path = dir.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    };
    w(
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    w("src/lib.rs", "mod a;\nmod b;\n");
    w("src/a.rs", "pub fn f() { crate::b::g() }\n");
    w("src/b.rs", b_src);
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    dir
}

#[test]
fn a_real_package_with_a_loop_is_reported_and_without_it_is_not() {
    let dir = repo("pub fn g() { crate::a::f() }\n");
    let (f, notes) = ModuleCyclesCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].location.as_ref().unwrap().file, "src/a.rs");
    assert!(notes.iter().any(|n| n.contains("1 cycle")), "{notes:?}");
    let dir = repo("pub fn g() {}\n");
    let (f, notes) = ModuleCyclesCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert!(f.is_empty(), "{f:?}");
    // positive control: the graph was read
    assert!(notes.iter().any(|n| n.contains("1 edge(s)")), "{notes:?}");
}
