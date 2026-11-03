# Package B: ARC-005 depend toward stability, MOD-009 long reach-through chains

Read [README](README.md) first. Both are Rust, advisory (Info, low confidence), and reuse `modgraph.rs` and the `[architecture]` components of `.coderipper.toml`.

**ARC-005** (`rules/arc.toml`): instability I = Ce / (Ca + Ce) per declared component, from the component graph that ARC-001 and ARC-002 already build
(`architecture/mod.rs`). Flag an allowed edge A -> B where I(B) > I(A) (depends on something less stable), ranked by the margin. Edges to a
component in `volatile` or to the standard library are not reported; cycles are ARC-002's. Opt-in threshold (`[arc005] min_margin`); pick a default that
leaves coderipper itself quiet (measure it). Patent read first: stability metrics (the stable-dependencies principle) are old; search "instability metric" tools.

**MOD-009**: a method-call chain longer than N (`a.b().c().d().e()`) that reaches through a value's structure. Builders and iterator adaptors (long chains
on one receiver type) are not reach-through: count only chains where each link moves to a field or accessor of a *different* struct declared in the crate.
Syntactic (`syn`), shapes only. Measure N = 3..5 on the corpus; expect many builder false positives, so the type-change test is the whole rule.
Patent read first: Law of Demeter detection; expected hits are lint tools, not claims.

Done when: both rules registered, fixtures, corpus numbers in the PR, patent rows, fragments, READY. Do ARC-005 first (smaller).
