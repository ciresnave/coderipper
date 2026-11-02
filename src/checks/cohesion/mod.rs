// SPDX-License-Identifier: MIT OR Apache-2.0
//! Usage clusters (project scope, Rust): which types hold methods that never touch the same state?
//!
//! Rule record: `MOD-004` in `rules/mod.toml` (the rule catalog). Background in `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.5.
//!
//! **What it measures** (read from `syn`, at `HEAD`, from the tracked `.rs` files under the package's `src/`; see `facts`): for each
//! struct, the methods of its inherent `impl` blocks that take `self`, which `self.field` each one uses and which sibling methods it
//! calls. Two methods are in one cluster when they use a common field, directly or through the siblings they call. A type is
//! reported when its methods fall into two or more clusters that each hold enough methods.
//!
//! **Weak evidence by construction (V1, low confidence):** a type whose methods use disjoint fields may be a bag of independent
//! helpers or may hold two jobs, and the check cannot tell which. The finding names the clusters so a person judges, and it never
//! gates: Info only.
//!
//! **Known limitations / deliberate scope:**
//! - Accessors and setters (one statement, at most one field, no call) are not members of any cluster: a struct with a getter per
//!   field would otherwise split into as many clusters as it has fields.
//! - A method that touches no field and calls no sibling is not clustered either.
//! - Syntactic (see `facts`): a field reached through a destructuring or passed on through a helper taking `&self.a` is not seen, so
//!   clusters can look more separate than they are.
//! - Methods in trait `impl`s are not members (the trait dictates their signatures). Test items are not read. Only structs; an
//!   `enum` has no fields to partition.
//! - Analysis is HEAD-only, one package at a time, as with the other checks.

mod facts;

use crate::checks::rust_sources::read_sources;
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use facts::{clusters, collect, Cluster, TypeFacts};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_004;

/// What makes a split worth reporting.
pub(crate) struct Params {
    /// Clusters needed.
    pub min_clusters: usize,
    /// Methods a cluster needs to count.
    pub min_cluster_methods: usize,
    /// Methods (outside accessors) the type needs in all.
    pub min_methods: usize,
    pub source: &'static str,
}

/// PLACEHOLDER until the corpus is measured.
pub(crate) const PARAMS: Params = Params {
    min_clusters: 2,
    min_cluster_methods: 2,
    min_methods: 4,
    source: "placeholder",
};

/// Reports types whose methods split into separate usage clusters (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct CohesionCheck;

impl CohesionCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for CohesionCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let (package, _) = crate::package::locate(&ctx.project_root)?;
        let files = read_sources(&package.dir)?;
        let (types, notes) = collect(&files)?;
        let project = package
            .dir
            .file_name()
            .map_or_else(|| package.name.clone(), |n| n.to_string_lossy().into_owned());
        let mut findings = judge(&types, &PARAMS, &project);
        findings.sort_by(|a, b| {
            let key = |f: &Finding| {
                f.location
                    .as_ref()
                    .map(|l| (l.file.clone(), l.line))
                    .unwrap_or_default()
            };
            key(a).cmp(&key(b)).then_with(|| a.subject.cmp(&b.subject))
        });
        Ok((findings, notes))
    }
}

/// The clusters that count under these parameters, or none when the type does not split.
fn splits<'a>(t: &TypeFacts, p: &Params, all: &'a mut Vec<Cluster>) -> Option<Vec<&'a Cluster>> {
    *all = clusters(t);
    let big: Vec<&Cluster> = all
        .iter()
        .filter(|c| c.methods.len() >= p.min_cluster_methods)
        .collect();
    let methods: usize = all.iter().map(|c| c.methods.len()).sum();
    (big.len() >= p.min_clusters && methods >= p.min_methods).then_some(big)
}

fn judge(types: &[TypeFacts], p: &Params, project: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in types {
        let mut all = Vec::new();
        let Some(mut big) = splits(t, p, &mut all) else {
            continue;
        };
        big.sort_by(|a, b| b.methods.len().cmp(&a.methods.len()));
        let describe = |c: &&Cluster| {
            let mut m = c.methods.clone();
            m.sort();
            format!(
                "[{}] over fields [{}]",
                m.join(", "),
                c.fields.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        };
        out.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Info,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: t.file.clone(),
                line: Some(t.line as u32),
            }),
            subject: Some(t.name.clone()),
            summary: format!(
                "the `{}`-named type splits into {} usage clusters of {} or more methods ({} methods in all, outside accessors)",
                t.name,
                big.len(),
                p.min_cluster_methods,
                all.iter().map(|c| c.methods.len()).sum::<usize>(),
            ),
            detail: format!(
                "Methods are in one cluster when they use a common field, directly or through the sibling methods they call. \
                 Clusters: {}. {} This is weak evidence on its own: the type may be a set of independent helpers, or it may hold \
                 more than one job. If the groups really never meet, split the type along them; if they belong together, leave it \
                 and say why in an allowlist reason. Syntactic: a field reached by destructuring or passed on through a helper is \
                 not seen, and accessors, trait-impl methods and test items are not counted.",
                big.iter().map(describe).collect::<Vec<_>>().join("; "),
                p.source,
            ),
            positive_control: None,
            member: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::rust_sources::SourceFile;

    const P: Params = Params {
        min_clusters: 2,
        min_cluster_methods: 2,
        min_methods: 4,
        source: "test parameters",
    };

    fn judged(src: &str) -> Vec<Finding> {
        let files = [SourceFile {
            path: "src/lib.rs".into(),
            text: src.into(),
        }];
        let (types, _) = collect(&files).unwrap();
        judge(&types, &P, "p")
    }

    const TWO_JOBS: &str = "pub struct S { a: u8, b: u8, c: u8, d: u8 }
impl S {
    fn x1(&self) -> u8 { self.a + self.b }
    fn x2(&mut self) { self.a = 1; self.b = 2; }
    fn y1(&self) -> u8 { self.c + self.d }
    fn y2(&mut self) { self.c = 3; self.d = 4; }
}";

    #[test]
    fn two_clusters_of_two_methods_are_reported_with_their_names() {
        let f = judged(TWO_JOBS);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Info);
        assert_eq!(f[0].confidence, Confidence::Low);
        assert!(f[0].detail.contains("[x1, x2] over fields [a, b]"), "{}", f[0].detail);
        assert!(f[0].detail.contains("[y1, y2] over fields [c, d]"), "{}", f[0].detail);
        assert!(f[0].summary.contains("2 usage clusters"), "{}", f[0].summary);
    }

    #[test]
    fn a_bridging_method_makes_it_one_cluster_and_no_finding() {
        let src = TWO_JOBS.replace("}\n}", "}\n    fn bridge(&self) -> u8 { self.a + self.c }\n}");
        assert!(judged(&src).is_empty());
    }

    #[test]
    fn a_cluster_below_the_method_floor_does_not_count() {
        // one cluster of two methods, a second of a single method: not two clusters
        let src = "pub struct S { a: u8, b: u8, c: u8 }
impl S {
    fn x1(&self) -> u8 { self.a + self.b }
    fn x2(&mut self) { self.a = 1; self.b = 2; }
    fn y1(&self) -> u8 { self.c + self.c }
    fn x3(&self) -> u8 { self.a + self.a }
}";
        assert!(judged(src).is_empty());
    }

    #[test]
    fn a_type_with_too_few_methods_in_all_is_not_reported() {
        let strict = Params { min_methods: 5, ..P };
        let files = [SourceFile { path: "src/lib.rs".into(), text: TWO_JOBS.into() }];
        let (types, _) = collect(&files).unwrap();
        assert!(judge(&types, &strict, "p").is_empty());
    }

    #[test]
    fn a_struct_of_getters_is_not_reported() {
        let src = "pub struct S { a: u8, b: u8, c: u8, d: u8 }
impl S {
    fn a(&self) -> u8 { self.a } fn set_a(&mut self, v: u8) { self.a = v; }
    fn b(&self) -> u8 { self.b } fn set_b(&mut self, v: u8) { self.b = v; }
    fn c(&self) -> u8 { self.c } fn set_c(&mut self, v: u8) { self.c = v; }
    fn d(&self) -> u8 { self.d } fn set_d(&mut self, v: u8) { self.d = v; }
}";
        assert!(judged(src).is_empty());
    }

    #[test]
    fn no_finding_reads_as_an_absence_claim_to_the_host() {
        let f = judged(&TWO_JOBS.replace("S", "None"));
        assert_eq!(f.len(), 1);
        let summary = f[0].summary.clone();
        f.into_iter().next().unwrap().validate().unwrap_or_else(|e| panic!("{summary}: {e}"));
    }
}
