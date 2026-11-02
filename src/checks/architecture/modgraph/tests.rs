// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn mods(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(m, f)| ((*m).to_string(), (*f).to_string()))
        .collect()
}

fn segs(s: &str) -> Vec<String> {
    s.split("::").map(str::to_string).collect()
}

#[test]
fn paths_resolve_through_crate_self_super_and_child_modules() {
    let m = mods(&[
        ("", "src/lib.rs"),
        ("a", "src/a.rs"),
        ("a::b", "src/a/b.rs"),
        ("c", "src/c.rs"),
    ]);
    assert_eq!(
        resolve(&m, "", &segs("crate::a::f")).as_deref(),
        Some("src/a.rs")
    );
    assert_eq!(
        resolve(&m, "a", &segs("b::g")).as_deref(),
        Some("src/a/b.rs")
    );
    assert_eq!(
        resolve(&m, "a::b", &segs("super::f")).as_deref(),
        Some("src/a.rs")
    );
    assert_eq!(
        resolve(&m, "a::b", &segs("super::super::c::h")).as_deref(),
        Some("src/c.rs")
    );
    assert_eq!(
        resolve(&m, "a", &segs("self::b::g")).as_deref(),
        Some("src/a/b.rs")
    );
    // the longest module path wins and the rest is an item
    assert_eq!(
        resolve(&m, "", &segs("crate::a::b::Kind::X")).as_deref(),
        Some("src/a/b.rs")
    );
    // `crate::Item` is an item of the root
    assert_eq!(
        resolve(&m, "a", &segs("crate::Thing")).as_deref(),
        Some("src/lib.rs")
    );
}

#[test]
fn a_path_into_another_crate_or_past_the_root_names_nothing() {
    let m = mods(&[("", "src/lib.rs"), ("a", "src/a.rs")]);
    assert_eq!(resolve(&m, "", &segs("serde::Serialize")), None);
    assert_eq!(resolve(&m, "a", &segs("std::fmt::Debug")), None);
    assert_eq!(resolve(&m, "", &segs("super::x")), None);
    // positive control: the same shape with a child module resolves
    assert!(resolve(&m, "", &segs("a::x")).is_some());
}

#[test]
fn edges_run_from_the_file_that_refers_to_the_file_it_refers_to() {
    let g = module_graph(&[
        file("src/lib.rs", "mod core;\nmod adapters;\nmod shell;\n"),
        file("src/core.rs", "pub fn plan() {}\n"),
        file(
            "src/adapters.rs",
            "use crate::core::plan;\npub fn go() { plan() }\n",
        ),
        file(
            "src/shell.rs",
            "pub fn run() { crate::adapters::go(); crate::core::plan(); }\n",
        ),
    ])
    .unwrap();
    let e: Vec<(&str, &str)> = g
        .edges
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        e,
        [
            ("src/adapters.rs", "src/core.rs"),
            ("src/shell.rs", "src/adapters.rs"),
            ("src/shell.rs", "src/core.rs")
        ]
    );
    assert_eq!(g.files.len(), 4);
}

#[test]
fn use_groups_globs_and_super_are_followed_and_a_declaration_is_not_an_edge() {
    let g = module_graph(&[
        file("src/lib.rs", "mod a;\nmod b;\n"),
        file("src/a.rs", "use crate::{b::X, c::Y};\nuse super::b::*;\n"),
        file("src/b.rs", "pub struct X;\n"),
    ])
    .unwrap();
    // `mod a; mod b;` in lib.rs are containment, not dependency; `c` is not a module
    let e: Vec<(&str, &str)> = g
        .edges
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(e, [("src/a.rs", "src/b.rs"), ("src/a.rs", "src/lib.rs")]);
}

#[test]
fn an_inline_module_belongs_to_the_file_it_is_written_in() {
    let g = module_graph(&[
        file("src/lib.rs", "mod inner { pub fn f() {} }\nmod x;\n"),
        file("src/x.rs", "fn g() { crate::inner::f() }\n"),
    ])
    .unwrap();
    let e: Vec<(&str, &str)> = g
        .edges
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(e, [("src/x.rs", "src/lib.rs")]);
}

#[test]
fn test_items_are_not_read_and_unreached_or_missing_files_are_noted() {
    let g = module_graph(&[
        file(
            "src/lib.rs",
            "mod a;\nmod gone;\n#[cfg(test)]\nmod tests { fn t() { crate::a::f() } }\n",
        ),
        file("src/a.rs", "pub fn f() {}\n"),
        file("src/orphan.rs", "pub fn o() {}\n"),
    ])
    .unwrap();
    assert!(g.edges.is_empty(), "{:?}", g.edges);
    assert!(g.notes.iter().any(|n| n.contains("gone")), "{:?}", g.notes);
    assert!(
        g.notes.iter().any(|n| n.contains("1 tracked source file")),
        "{:?}",
        g.notes
    );
}

#[test]
fn a_library_with_a_binary_reads_the_library_and_says_so() {
    let g = module_graph(&[
        file("src/lib.rs", "pub fn f() {}\n"),
        file("src/main.rs", "fn main() {}\n"),
    ])
    .unwrap();
    assert!(
        g.notes.iter().any(|n| n.contains("src/main.rs")),
        "{:?}",
        g.notes
    );
    // positive control: a binary alone is the root and says nothing about itself
    let g = module_graph(&[
        file("src/main.rs", "mod a;\nfn main() {}\n"),
        file("src/a.rs", ""),
    ])
    .unwrap();
    assert!(g.notes.is_empty(), "{:?}", g.notes);
    assert_eq!(g.files.len(), 2);
}

#[test]
fn an_unparsable_file_is_an_error_never_a_smaller_graph() {
    assert!(module_graph(&[file("src/lib.rs", "mod a;"), file("src/a.rs", "fn (")]).is_err());
}

#[test]
fn paths_entries_match_files_directories_and_patterns() {
    assert!(matches_path("src/plan.rs", "src/plan.rs"));
    assert!(!matches_path("src/plan.rs", "src/plan.rs.bak"));
    assert!(matches_path("src/billing", "src/billing/mod.rs"));
    assert!(matches_path("src/billing/**", "src/billing/a/b.rs"));
    assert!(!matches_path("src/billing/**", "src/billing.rs"));
    assert!(matches_path("src/*.rs", "src/a.rs"));
    assert!(!matches_path("src/*.rs", "src/a/b.rs"));
    assert!(matches_path("src/**/*.rs", "src/a/b/c.rs"));
}

fn arch(toml_text: &str) -> ArchitectureConfig {
    crate::config::ConfigFile::parse(toml_text, std::path::Path::new("c.toml"))
        .unwrap()
        .architecture
        .unwrap()
}

#[test]
fn the_longest_paths_entry_places_a_file() {
    let a = arch(
        "[architecture]\n[[architecture.component]]\nname = \"all\"\npaths = [\"src/**\"]\n[[architecture.component]]\nname = \"core\"\npaths = [\"src/core.rs\", \"src/core/**\"]\n",
    );
    assert_eq!(component_of(&a, "src/core.rs"), Some("core"));
    assert_eq!(component_of(&a, "src/core/x.rs"), Some("core"));
    assert_eq!(component_of(&a, "src/other.rs"), Some("all"));
    assert_eq!(component_of(&a, "build.rs"), None);
}

fn repo(core_src: &str) -> tempfile::TempDir {
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
    w("src/lib.rs", "mod core;\nmod adapters;\nmod shell;\n");
    w("src/core.rs", core_src);
    w(
        "src/adapters.rs",
        "use crate::core::plan;\npub fn go() { plan() }\n",
    );
    w("src/shell.rs", "pub fn run() { crate::adapters::go(); }\n");
    w(
        ".coderipper.toml",
        "[architecture]\n[[architecture.component]]\nname = \"core\"\npaths = [\"src/core.rs\"]\n[[architecture.component]]\nname = \"adapters\"\npaths = [\"src/adapters.rs\"]\n[[architecture.component]]\nname = \"shell\"\npaths = [\"src/shell.rs\", \"src/lib.rs\"]\n[[architecture.layer]]\nname = \"bottom\"\ncomponents = [\"core\"]\n[[architecture.layer]]\nname = \"middle\"\ncomponents = [\"adapters\"]\n[[architecture.layer]]\nname = \"top\"\ncomponents = [\"shell\"]\n",
    );
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

fn judged(dir: &tempfile::TempDir) -> (Vec<crate::finding::Finding>, Vec<String>) {
    use crate::check::{Check, CheckContext};
    super::super::ComponentGraphCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap()
}

#[test]
fn a_single_crate_split_into_module_groups_is_judged_from_its_source() {
    // core refers up to adapters: a violation of the bottom/middle/top layers
    let dir = repo("pub fn plan() { crate::adapters::go() }\n");
    let (f, notes) = judged(&dir);
    let subjects: Vec<_> = f.iter().filter_map(|x| x.subject.clone()).collect();
    assert_eq!(subjects, ["core->adapters"], "{f:?}");
    assert!(
        notes
            .iter()
            .any(|n| n.contains("module graph of `app`: 4 file(s)")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("module group `core`: 1 source file(s)")),
        "{notes:?}"
    );
}

#[test]
fn the_same_crate_with_core_clean_has_no_finding() {
    let dir = repo("pub fn plan() {}\n");
    let (f, notes) = judged(&dir);
    assert!(f.is_empty(), "{f:?}");
    // positive control: the graph was read (edges exist), it is not empty because nothing ran
    assert!(
        notes.iter().any(|n| n.contains("2 file-to-file edge(s)")),
        "{notes:?}"
    );
}
