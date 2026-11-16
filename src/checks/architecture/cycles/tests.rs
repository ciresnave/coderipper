// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::architecture::ArchitectureConfig;
use crate::checks::architecture::{Edge, Member};

fn set(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| (*s).to_string()).collect()
}

fn es(xs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    xs.iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn a_strongly_connected_set_is_found_and_a_chain_is_not() {
    let nodes = set(&["a", "b", "c", "d"]);
    // a -> b -> c -> a is a loop; d hangs off it
    let loops = sccs(
        &nodes,
        &es(&[("a", "b"), ("b", "c"), ("c", "a"), ("c", "d")]),
    );
    assert_eq!(loops, [["a", "b", "c"]]);
    // positive control: the same nodes in a chain have none
    assert!(sccs(&nodes, &es(&[("a", "b"), ("b", "c"), ("c", "d")])).is_empty());
    // two separate loops are two sets
    let two = sccs(
        &set(&["a", "b", "c", "d"]),
        &es(&[("a", "b"), ("b", "a"), ("c", "d"), ("d", "c")]),
    );
    assert_eq!(two, [["a", "b"], ["c", "d"]]);
}

#[test]
fn every_single_edge_that_dissolves_a_loop_is_named() {
    let scc: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    let (cut, single) = dissolving(&scc, &es(&[("a", "b"), ("b", "c"), ("c", "a")]));
    assert!(single);
    assert_eq!(cut.len(), 3, "{cut:?}");
}

#[test]
fn when_no_single_edge_is_enough_a_depth_first_set_is_given() {
    // two loops sharing the node a: a<->b and a<->c, so removing one edge leaves the other
    let scc: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
    let all = es(&[("a", "b"), ("b", "a"), ("a", "c"), ("c", "a")]);
    let (cut, single) = dissolving(&scc, &all);
    assert!(!single);
    let rest: BTreeSet<_> = all.iter().filter(|e| !cut.contains(e)).cloned().collect();
    assert!(
        acyclic(&scc, &rest),
        "the set must dissolve the cycle: {cut:?}"
    );
}

fn arch(text: &str) -> ArchitectureConfig {
    crate::config::ConfigFile::parse(text, std::path::Path::new("c.toml"))
        .unwrap()
        .architecture
        .unwrap()
}

fn graph(members: &[&str], edges: &[(&str, &str)]) -> Graph {
    Graph {
        members: members
            .iter()
            .map(|n| Member {
                name: (*n).into(),
                dir: (*n).into(),
                manifest: format!("{n}/Cargo.toml"),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|(f, t)| Edge {
                from: (*f).into(),
                to: (*t).into(),
            })
            .collect(),
        ..Graph::default()
    }
}

// components: ui = crates u1,u2; core = crate c1 (u1 -> c1 -> u2 makes ui and core depend on each other)
const TOML: &str = "[architecture]
[[architecture.component]]
name = \"ui\"
paths = [\"u1\", \"u2\"]
[[architecture.component]]
name = \"core\"
paths = [\"c1\"]
";

#[test]
fn two_components_that_reach_each_other_through_their_crates_are_one_finding() {
    let g = graph(&["u1", "u2", "c1"], &[("u1", "c1"), ("c1", "u2")]);
    let (f, notes) = judge(&arch(TOML), &g, "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("core,ui"));
    assert_eq!(f[0].severity, Severity::Medium);
    assert_eq!(f[0].confidence, Confidence::High);
    assert!(f[0].summary.starts_with("2 components"), "{}", f[0].summary);
    assert!(f[0].detail.contains("`ui` -> `core`") && f[0].detail.contains("`core` -> `ui`"));
    assert!(notes[0].contains("1 cycle"), "{notes:?}");
}

#[test]
fn a_one_way_dependency_between_components_is_no_cycle() {
    let g = graph(&["u1", "u2", "c1"], &[("u1", "c1"), ("u2", "c1")]);
    let (f, notes) = judge(&arch(TOML), &g, "p");
    assert!(f.is_empty(), "{f:?}");
    // positive control: the edges were read
    assert!(
        notes[0].contains("2 component(s) and 1 edge(s)"),
        "{notes:?}"
    );
}

#[test]
fn a_module_group_in_the_set_lowers_the_confidence() {
    let mut g = graph(&["u1", "u2", "c1"], &[("u1", "c1"), ("c1", "u2")]);
    g.module_groups.insert("core".into(), 2);
    let (f, _) = judge(&arch(TOML), &g, "p");
    assert_eq!(f[0].confidence, Confidence::Medium);
}

#[test]
fn not_run_names_what_to_add() {
    assert!(missing_declaration(None)
        .unwrap()
        .contains("no [architecture] table"));
    let empty = arch("[architecture]\nstyle = \"layered\"\n");
    assert!(missing_declaration(Some(&empty))
        .unwrap()
        .contains("no components or layers"));
    assert!(missing_declaration(Some(&arch(TOML))).is_none());
}

fn repo(adapters_src: &str) -> tempfile::TempDir {
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
    w("src/lib.rs", "mod core;\nmod adapters;\n");
    w("src/core.rs", "pub fn plan() { crate::adapters::go() }\n");
    w("src/adapters.rs", adapters_src);
    w(
        ".coderipper.toml",
        "[architecture]\n[[architecture.component]]\nname = \"core\"\npaths = [\"src/core.rs\", \"src/lib.rs\"]\n[[architecture.component]]\nname = \"adapters\"\npaths = [\"src/adapters.rs\"]\n",
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

#[test]
fn module_groups_that_refer_to_each_other_are_a_cycle_in_a_real_repository() {
    let dir = repo("pub fn go() { crate::core::plan() }\n");
    let (f, _) = ComponentCyclesCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("adapters,core"));
    assert_eq!(f[0].confidence, Confidence::Medium);
    assert_eq!(f[0].location.as_ref().unwrap().file, "Cargo.toml");
}

#[test]
fn the_same_repository_without_the_back_reference_has_no_cycle() {
    let dir = repo("pub fn go() {}\n");
    let (f, notes) = ComponentCyclesCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert!(f.is_empty(), "{f:?}");
    // positive control: core -> adapters is still an edge
    assert!(notes[0].contains("1 edge(s)"), "{notes:?}");
}
