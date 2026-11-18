# MOD-011 call-graph lens: scope and slice plan

**Status:** plan (no code). **Ask:** CireSnave 2026-11-17, via the PM: *"Option 2 — call-graph lens, coderipper lane, as a MOD-011 extension."*
**Oracle:** Pip's spike report, section "Validation vs the audit" (`analysant/reference/pip-spike-2026-11-16/`), the three rows where the spike's lens
disagreed with the audit: `pick_route`, `capability_gated_rules`, `optimize_to_fixpoint`. The other four rows are doc-vs-code, not MOD-011.

## MOD-011 today (0.47.12, main 2c7a08d)

Ships, lifecycle advisory, alias `reachability`, numbered 2026-11-15 (0.46.0). The dead verdict is rustc's `dead_code` lint in a throwaway worktree of
HEAD with every top-level `pub` item narrowed to `pub(crate)`; callers outside the library (bins, tests, examples, benches, path-dependent members) and the
Info/Medium grading are **bare-identifier** matches; a documented `pub` item counts as live when working out what else is dead.

## Real-Fuel measurement (Fuel origin/main 6b66664d3258d5c1349f5878796e2659339bcf92, read-only scratch worktree; the 0.47.12 binary, `--no-config`)

| case | audit (b0d74ef) | call sites at 6b66664 (grep plus `#[cfg(test)]` line markers, so a reading, not a proof) | MOD-011 today | agree? |
| --- | --- | --- | --- | --- |
| `pick_route`, `RouteCache` | STUB: not called on the realize path | `pick_route` is called in production only from `RouteCache::resolve` (route_picker.rs:182); `RouteCache` appears only in a `pub use` and in tests | no finding for either | **no** (silent) |
| `LOOKAHEAD_K` (same row) | STUB: defined and re-exported, unused | defined route_picker.rs:81, re-exported, no use | Info "documented library API, zero callers" | yes |
| `default_rules`, `capability_gated_rules` | ABSENT on realize | `default_rules` has test callers only; `capability_gated_rules` has one production caller, `default_rules` | no finding for either | **no** (silent) |
| `optimize_to_fixpoint` | ABSENT on realize | production callers only through `lower_to_base_map` (opt.rs:377), itself called from `runtime_fused.rs`, `jit_ingest*.rs` | no finding | **no**, and not expressible: nothing names a *realize* entry point |

On a seeded free-function crate of the same shape the shipped check reports the dead chain (Medium) but not `pick_route`, because the only caller is a
documented `pub` method, which counts as live. Why the real crate is silent on the chain too (the same shape, as methods of a `pub` type) is not explained yet: slice 0.

## Design

- The audit's question is not "is there a production caller" but "is it reachable from the **realize** path". That is reachability from a **declared root set**,
  so roots cannot be inferred: `pick_route` is live from `RouteCache::resolve` and `resolve` is documented API.
- **Default mode is unchanged** (documented `pub` counts as live; library API stays library API; no new findings without configuration).
- **Rooted mode** is opt-in. Roots come from the repository's own `.coderipper.toml`, visible like `[reachability] public_api` is:
  `[reachability.roots.<name>] items = ["crate::path::Item", ...]` (crate-qualified paths; an unresolvable path is an **error**, never an empty root set).
  In rooted mode `pub` is not a root unless declared, and calls from `#[cfg(test)]`, `#[test]`, `tests/` and `benches/` are never edges.
  Whoever knows the entry points writes them (for Fuel, the Fuel lane: realize, jit); CodeRipper does not guess.
- Output: for an item, the root sets it is reachable from, and a finding (Info, advisory) when a repository's own expectation says it should be reachable
  from a set and is not, or is reachable only from another. Identity is path-qualified, so `Rule::matches` and an unrelated `matches` are two items.

## Slices (one lane branch; one PR per day, not before the waiver PR #364 lands, and not on 11-19 or 11-20)

| slice | closes | sabotage test (must fail without the change) |
| --- | --- | --- |
| 0. Tooling spike, no PR: run the three cases at Fuel with (A) rustc narrowing per crate, chained across crates, and (B) rust-analyzer call hierarchy; and find why the shipped check is silent on the chain | the "how" decision, with a measured table | n/a: a measurement; the deliverable is the table and a recommendation |
| 1. Root-set config: schema, parser, path resolution, errors | nothing alone (plumbing) | a typo'd path in a root set must fail the run by name; if it reads as an empty set the sabotage is "nothing reported" and the test fails |
| 2. Single-crate rooted analysis (test edges cut, pub not a root) | cases 1 and 2 when the roots are in the same crate | a seeded `resolve` with a production call from the root keeps `pick_route` live (finding disappears); the same call written in a `#[test]` does not rescue it (finding stays) |
| 3. Cross-crate edges (per slice 0) | the real Fuel cases, whose roots are in other crates | delete one cross-crate edge in a fixture workspace: the dependent item flips to dead |
| 4. Two root sets compared | case 3: reachable from `jit`, not from `realize` | add one `realize` to `optimize_to_fixpoint` edge: the finding disappears |
| 5. Path-qualified item facts in `--message-format json` for analysant | the "236 BUILT is an upper bound inflated by generic-word matches" half: analysant joins on paths instead of bare words | a fixture with two unrelated `matches` and one live caller: exactly one is live |
| 6. Acceptance: the Fuel table against the oracle rows, README and CHANGELOG | "agrees with the audit on all three" | the table is produced by the run, not typed |

Order 0, 1+2, 3, 4+5, 6. Earliest PR day for the first lens PR is 2026-11-21 (11-19 is coderipper-c's, 11-20 the gitleaks tuning PR) unless the PM moves a day.

## Open decisions (the PM, or CireSnave)

1. Roots live in the repository's `.coderipper.toml` (recommended), not a file per crate: one declaration per workspace, reviewed with the code.
2. Rooted mode keeps the documented-`pub` rule only as the default mode's; in rooted mode a declared root is the only thing that makes `pub` live.
3. Slice 0 may recommend a dependency (rust-analyzer's `ra_ap_*`): heavy, but exact on method calls. If so that is an [ASK] before slice 3.
4. The 236 recount itself is analysant's: this plan gives it path-qualified facts (slice 5), not the recount.
