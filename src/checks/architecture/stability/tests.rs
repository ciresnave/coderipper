// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use crate::checks::architecture::{Edge, Member};

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

fn arch(text: &str) -> ArchitectureConfig {
    crate::config::ConfigFile::parse(text, std::path::Path::new("c.toml"))
        .unwrap()
        .architecture
        .unwrap()
}

// x and y depend on a (a: Ca 2, Ce 1, I 1/3); a depends on b (b: Ca 1, Ce 2, I 2/3); b depends on p and q (I 0)
const MEMBERS: [&str; 6] = ["x", "y", "a", "b", "p", "q"];
const EDGES: [(&str, &str); 5] = [("x", "a"), ("y", "a"), ("a", "b"), ("b", "p"), ("b", "q")];

#[test]
fn instability_is_efferent_over_the_sum() {
    assert_eq!(instability(0, 0), None);
    assert_eq!(instability(3, 0), Some(0.0));
    assert_eq!(instability(0, 2), Some(1.0));
    assert_eq!(instability(2, 1), Some(1.0 / 3.0));
}

#[test]
fn an_edge_toward_a_less_stable_component_is_reported_with_its_margin() {
    let g = graph(&MEMBERS, &EDGES);
    let (f, notes) = judge(None, &g, 0.3, "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("a->b"));
    assert_eq!(f[0].severity, Severity::Info);
    assert_eq!(f[0].confidence, Confidence::Low);
    assert!(f[0].summary.contains("margin of 0.33"), "{}", f[0].summary);
    assert!(
        notes[0].contains("6 component(s) and 5 edge(s)"),
        "{notes:?}"
    );
    assert!(notes[0].contains("1 reported"), "{notes:?}");
}

#[test]
fn a_margin_above_the_gap_reports_nothing_and_the_same_graph_reported_it_below() {
    let g = graph(&MEMBERS, &EDGES);
    // positive control: the 0.3 run above reports it; 0.4 is above 1/3
    assert!(judge(None, &g, 0.4, "p").0.is_empty());
    assert_eq!(judge(None, &g, 1.0 / 3.0, "p").0.len(), 1);
}

#[test]
fn a_dependency_toward_a_more_stable_component_is_clean() {
    // x -> a -> c: every edge points to something with lower instability
    let g = graph(&["x", "a", "c"], &[("x", "a"), ("a", "c")]);
    let (f, notes) = judge(None, &g, 0.1, "p");
    assert!(f.is_empty(), "{f:?}");
    assert!(notes[0].contains("2 edge(s)"), "{notes:?}");
}

#[test]
fn findings_are_ranked_by_margin() {
    // a -> b has margin 1/3; c -> d has Ca 0 (I 1) and d has Ca 1, Ce 0... build a larger gap: s -> t where s is depended on by many
    let g = graph(
        &["u1", "u2", "u3", "s", "t", "z1", "z2", "z3", "a", "b", "p"],
        &[
            ("u1", "s"),
            ("u2", "s"),
            ("u3", "s"),
            ("s", "t"),
            ("t", "z1"),
            ("t", "z2"),
            ("t", "z3"),
            ("u1", "a"),
            ("a", "b"),
            ("b", "p"),
        ],
    );
    let (f, _) = judge(None, &g, 0.3, "p");
    let subjects: Vec<_> = f.iter().filter_map(|x| x.subject.as_deref()).collect();
    assert_eq!(subjects.first(), Some(&"s->t"), "{subjects:?}");
    assert!(f.windows(2).all(|w| {
        // the summaries carry the margin to two places: descending
        let m = |x: &Finding| {
            x.summary
                .rsplit("margin of ")
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap()
        };
        m(&w[0]) >= m(&w[1])
    }));
}

const TOML: &str = "[architecture]
[[architecture.component]]
name = \"x\"
paths = [\"x\"]
[[architecture.component]]
name = \"y\"
paths = [\"y\"]
[[architecture.component]]
name = \"a\"
paths = [\"a\"]
[[architecture.component]]
name = \"b\"
paths = [\"b\"]
volatility = \"volatile\"
[[architecture.component]]
name = \"p\"
paths = [\"p\"]
[[architecture.component]]
name = \"q\"
paths = [\"q\"]
";

#[test]
fn an_edge_to_a_component_declared_volatile_is_left_out() {
    let g = graph(&MEMBERS, &EDGES);
    let (f, notes) = judge(Some(&arch(TOML)), &g, 0.3, "p");
    assert!(f.is_empty(), "{f:?}");
    assert!(
        notes[0].contains("1 edge(s) to a volatile component"),
        "{notes:?}"
    );
    // positive control: without the tag the same declaration reports it
    let untagged = TOML.replace("volatility = \"volatile\"\n", "");
    let (f, _) = judge(Some(&arch(&untagged)), &g, 0.3, "p");
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("a->b"));
}

#[test]
fn an_edge_inside_a_cycle_is_left_to_arc_002() {
    // a <-> b is a cycle; with x and y above a and p, q below b, a -> b would otherwise read as a margin of 1/3
    let mut edges = EDGES.to_vec();
    edges.push(("b", "a"));
    let g = graph(&MEMBERS, &edges);
    let (f, notes) = judge(None, &g, 0.3, "p");
    assert!(
        f.iter().all(|x| x.subject.as_deref() != Some("a->b")),
        "{f:?}"
    );
    assert!(notes[0].contains("inside a cycle"), "{notes:?}");
}

#[test]
fn no_edges_is_said_not_passed() {
    let (f, notes) = judge(None, &graph(&["only"], &[]), 0.3, "p");
    assert!(f.is_empty());
    assert!(
        notes.iter().any(|n| n.contains("judged nothing")),
        "{notes:?}"
    );
}

#[test]
fn the_margin_is_read_from_the_repository_file() {
    assert_eq!(
        crate::config::arc005_min_margin("[arc005]\nmin_margin = 0.5\n").unwrap(),
        Some(0.5)
    );
    assert_eq!(crate::config::arc005_min_margin("").unwrap(), None);
    assert!(crate::config::arc005_min_margin("[arc005]\nmin_margin = 0.0\n").is_err());
    assert!(crate::config::arc005_min_margin("[arc005]\nmin_margin = 1.5\n").is_err());
}
