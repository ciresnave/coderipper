// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::architecture::ArchitectureConfig;

fn arch(toml: &str) -> ArchitectureConfig {
    crate::config::ConfigFile::parse(toml, Path::new(".coderipper.toml"))
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
        dev_edges: 0,
    }
}

const LAYERS: &str = "[architecture]\n[[architecture.layer]]\nname = \"low\"\ncomponents = [\"core\"]\n[[architecture.layer]]\nname = \"mid\"\ncomponents = [\"store\", \"net\"]\n[[architecture.layer]]\nname = \"top\"\ncomponents = [\"app\"]\n";

fn today() -> Date {
    Date::parse("2026-11-02").unwrap()
}

fn run(extra: &str, edges: &[(&str, &str)]) -> (Vec<Finding>, Vec<String>) {
    let a = arch(&format!("{LAYERS}{extra}"));
    let g = graph(&["core", "store", "net", "app"], edges);
    let names = g.members.iter().map(|m| m.name.clone()).collect();
    a.validate(&names, Path::new("c")).unwrap();
    judge(&a, &g, today(), "p")
}

fn subjects(f: &[Finding]) -> Vec<String> {
    f.iter().filter_map(|f| f.subject.clone()).collect()
}

#[test]
fn edges_that_point_down_or_sideways_are_fine() {
    let (f, notes) = run(
        "",
        &[
            ("app", "store"),
            ("store", "core"),
            ("app", "core"),
            ("store", "net"),
        ],
    );
    assert!(f.is_empty(), "{f:?}");
    assert!(
        notes
            .last()
            .unwrap()
            .contains("judged 4 dependency edge(s)"),
        "{notes:?}"
    );
}

#[test]
fn an_edge_that_points_up_is_a_finding_naming_both_layers() {
    let (f, _) = run("", &[("core", "app")]);
    assert_eq!(subjects(&f), ["core->app"]);
    assert_eq!(f[0].severity, Severity::Low);
    assert!(
        f[0].detail.contains("layer `low`") && f[0].detail.contains("layer `top`"),
        "{}",
        f[0].detail
    );
    assert_eq!(f[0].location.as_ref().unwrap().file, "core/Cargo.toml");
    f[0].clone().validate().unwrap();
}

#[test]
fn an_allow_edge_permits_an_upward_edge() {
    let allow =
        "[[architecture.allow]]\nfrom = \"core\"\nto = \"app\"\nreason = \"health probe\"\n";
    let (f, _) = run(allow, &[("core", "app")]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn an_unexpired_waiver_is_a_note_on_every_run_and_not_a_finding() {
    let w = "[[architecture.waiver]]\nedge = [\"core\", \"app\"]\nreason = \"r1\"\nowner = \"o1\"\nuntil = \"2026-11-02\"\n";
    let (f, notes) = run(w, &[("core", "app")]);
    assert!(f.is_empty(), "{f:?}");
    let waived = notes
        .iter()
        .find(|n| n.starts_with("waived:"))
        .expect("a waived note");
    assert!(
        waived.contains("r1") && waived.contains("o1") && waived.contains("2026-11-02"),
        "{waived}"
    );
}

#[test]
fn an_expired_waiver_no_longer_covers_the_edge() {
    let w = "[[architecture.waiver]]\nedge = [\"core\", \"app\"]\nreason = \"r1\"\nowner = \"o1\"\nuntil = \"2026-11-01\"\n";
    let (f, notes) = run(w, &[("core", "app")]);
    assert_eq!(subjects(&f), ["core->app"]);
    assert!(
        f[0].detail.contains("ended on 2026-11-01"),
        "{}",
        f[0].detail
    );
    assert!(!notes.iter().any(|n| n.starts_with("waived:")));
}

#[test]
fn a_waiver_for_an_edge_that_is_fine_is_stale() {
    let w = "[[architecture.waiver]]\nedge = [\"app\", \"core\"]\nreason = \"r\"\nowner = \"o\"\nuntil = \"2027-01-01\"\n";
    let (f, _) = run(w, &[("app", "core")]);
    assert_eq!(subjects(&f), ["waiver:app->core"]);
    assert_eq!(f[0].severity, Severity::Info);
    f[0].clone().validate().unwrap();
}

#[test]
fn a_member_in_no_layer_is_unplaced_and_its_edges_are_not_judged() {
    let a = arch(LAYERS);
    let g = graph(
        &["core", "store", "net", "app", "extra"],
        &[("core", "extra")],
    );
    let (f, notes) = judge(&a, &g, today(), "p");
    assert_eq!(subjects(&f), ["unplaced:extra"]);
    f[0].clone().validate().unwrap();
    assert!(
        notes
            .last()
            .unwrap()
            .contains("1 edge(s) touch a member the declaration does not place"),
        "{notes:?}"
    );
}

#[test]
fn a_declared_component_that_names_no_member_is_stale() {
    let c = "[[architecture.component]]\nname = \"gone\"\npaths = [\"crates/gone\"]\n";
    let (f, _) = run(c, &[]);
    assert_eq!(subjects(&f), ["stale:gone"]);
    f[0].clone().validate().unwrap();
}

#[test]
fn a_component_over_a_crate_directory_stands_for_that_crate() {
    let a = arch("[architecture]\n[[architecture.component]]\nname = \"base\"\npaths = [\"crates/core\"]\n[[architecture.layer]]\nname = \"low\"\ncomponents = [\"base\"]\n[[architecture.layer]]\nname = \"top\"\ncomponents = [\"app\"]\n");
    let mut g = graph(&["core", "app"], &[("base_member", "app")]);
    g.members[0].dir = "crates/core".into();
    g.members[0].name = "base_member".into();
    // the crate `base_member` lives in crates/core, so it is the component `base`, in the low layer, and depends upward on `app`
    let (f, _) = judge(&a, &g, today(), "p");
    assert_eq!(subjects(&f), ["base_member->app"], "{f:?}");
}

#[test]
fn module_paths_are_noted_and_not_judged() {
    let a = arch("[architecture]\n[[architecture.component]]\nname = \"inner\"\npaths = [\"core/src/billing/**\"]\n[[architecture.layer]]\nname = \"low\"\ncomponents = [\"core\"]\n");
    let g = graph(&["core"], &[]);
    let (f, notes) = judge(&a, &g, today(), "p");
    assert!(f.is_empty(), "{f:?}");
    assert!(
        notes
            .iter()
            .any(|n| n.contains("module groups are not judged")),
        "{notes:?}"
    );
}

#[test]
fn metadata_gives_path_edges_between_members_and_leaves_dev_edges_out() {
    let json = serde_json::json!({"packages": [
        {"name": "a", "manifest_path": "/w/a/Cargo.toml", "dependencies": [
            {"name": "b", "path": "/w/b", "kind": null},
            {"name": "c", "path": "/w/c", "kind": "dev"},
            {"name": "serde", "kind": null},
            {"name": "b-by-version", "kind": null}]},
        {"name": "b", "manifest_path": "/w/b/Cargo.toml", "dependencies": []},
        {"name": "c", "manifest_path": "/w/c/Cargo.toml", "dependencies": [{"name": "b", "path": "/w/b", "kind": "build"}]}
    ]});
    let g = graph_from_metadata(&json, Path::new("/w")).unwrap();
    assert_eq!(
        g.edges
            .iter()
            .map(|e| format!("{}->{}", e.from, e.to))
            .collect::<Vec<_>>(),
        ["a->b", "c->b"]
    );
    assert_eq!(g.dev_edges, 1);
    assert_eq!(
        g.members.iter().map(|m| m.dir.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
}

#[test]
fn a_repository_with_no_declaration_is_not_run_and_never_clean() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = CheckContext::new(dir.path());
    let why = ComponentGraphCheck::new()
        .not_configured(&ctx)
        .expect("not configured");
    assert!(
        why.contains("[architecture]") && why.contains("not a pass"),
        "{why}"
    );
    std::fs::write(
        dir.path().join(".coderipper.toml"),
        "[architecture]\nstyle = \"layered\"\n",
    )
    .unwrap();
    let why = ComponentGraphCheck::new()
        .not_configured(&ctx)
        .expect("declares no layers");
    assert!(why.contains("no layers"), "{why}");
    std::fs::write(dir.path().join(".coderipper.toml"), LAYERS).unwrap();
    assert!(ComponentGraphCheck::new().not_configured(&ctx).is_none());
}
