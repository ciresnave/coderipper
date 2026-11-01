# ARC and MOD checks: design note

**Status: proposed, docs only (2026-11-01).** Task from the PM, relaying CireSnave verbatim: *"If CodeRipper doesn't have checks for
most of its rules even just for Rust code, we need to get those written. Those in its ARC and MOD families are very important to me."*
This note fixes what each rule measures before any check is written. **Nothing here is built.** Judgment calls are marked **[ASK]**
and go to the PM before the check that depends on them.

Evidence labels: **MEASURED** = run in this session, ref named. **ASSUMED** = reasoned, not checked. Unlabelled = a decision.

## 0. State today (MEASURED, origin/main ee8c579)

`docs/rules/arc.md`: 0 of 12 rules have a check. `docs/rules/mod.md`: MOD-001 is partly covered by `reachability`, the other nine have none.
Classes: **P** parser-level facts (buildable now), **A** needs a declared architecture, **H** needs history, **L** needs a model.

| Class | ARC | MOD |
| :-- | :-- | :-- |
| P | 004, 005, 008, 012 | 001, 002, 003, 004, 005, 008, 009, 010 |
| A | 001, 002, 003, 006, 007 | (010 needs a tag) |
| H | 009, 011 | 006 |
| L | 010 | 007 |

## 1. Common ground

**Data sources.** (a) `cargo metadata` for the crate graph (exact; it can run build tooling, so run it `--offline` and per the
existing build-cache rules). (b) `syn` (already a dependency, 3.x, `full` + `visit`) over `git ls-tree origin/main`-style tracked
files, never a disk walk (portfolio rule 5b). (c) `rustc`/rust-analyzer type facts only where a rule says so (ARC-004); until then
those rules state that they see syntax, not types.

**What `syn` cannot see, said in every finding's "cannot see" line:** macro-generated items, `cfg`-dependent modules other than
the default feature set, glob-import targets, trait-method dispatch, `#[path]` modules when the path is not literal. A module graph
built from `mod` declarations plus `use`/path expressions is an under-approximation; a cycle it reports is real, a cycle it misses is
not ruled out. Findings are therefore **evidence of presence only**; no rule claims "no cycles".

**Test code.** Items under `#[cfg(test)]`, files under `tests/`, `benches/`, `examples/` and `build.rs` are excluded from size,
coupling and surface counts (they are a different population: Fuel's inline test modules inflate whole-file line counts).
This is a classification of *kind of code*, not a path-wide exclusion of a rule; it is stated in each finding.

**Severity.** Advisory: `Info` for a metric outlier, `Low` where a structural fact is certain (a cycle), never gating by default.
**Default ON** unless the measured false-positive count on the corpus is high (CireSnave 285). **No path-wide exclusions**: an
exception is a per-finding waiver with reason, owner and end date, as for every other rule.

**Finding text** always carries: the value, the threshold, where the threshold came from, and the fix (one concrete step).

**Quality bar per rule (all in the PR):** conformance fixtures (a seeded defect caught, a clean twin passes); a mutation check
(the mutation must compile and be one that could ship, predicted output shape asserted); false positives *counted by reading*, on the
corpus, reported in the PR; catalogue row changed from "no check yet" to the check id only once the fixtures earn it.

## 2. The corpus

Fuel, auth-framework, Unpopped, KISS, PersistAnt, configurant, httpant, releaseant, coderipper. Source files at each repo's
`origin/main` (blobs, not working trees), production Rust only (excluding the test kinds above, by path; inline `#[cfg(test)]`
is *not* removed in these line counts, so they overstate):

| repo @ origin/main | prod .rs files | lines | p50 | p90 | p95 | p99 | max | files >3,000 | >1,000 |
| :-- | --: | --: | --: | --: | --: | --: | :-- | --: | --: |
| fuel @3a41200ce | 651 | 536,573 | 457 | 1,403 | 2,334 | 11,903 | 17,237 fuel-graph/src/lib.rs | 22 | 129 |
| auth-framework @24e45ca7 | 234 | 186,586 | 646 | 1,761 | 2,035 | 3,114 | 5,981 | 3 | 60 |
| Unpopped @019050a | 68 | 67,403 | 402 | 2,004 | 4,795 | 7,696 | 8,380 | 4 | 18 |
| KISS @d4fa3d1 | 65 | 21,430 | 221 | 840 | 1,105 | 1,407 | 1,429 | 0 | 5 |
| PersistAnt @7590b33 | 87 | 31,004 | 268 | 665 | 1,009 | 1,541 | 1,915 | 0 | 5 |
| configurant @dd7a331 | 37 | 12,753 | 227 | 740 | 895 | 1,197 | 1,197 | 0 | 1 |
| httpant @466d8ad | 48 | 12,977 | 212 | 549 | 564 | 1,268 | 1,268 | 0 | 1 |
| releaseant @c924154 | 22 | 8,490 | 279 | 770 | 850 | 1,503 | 1,503 | 0 | 1 |
| coderipper @ee8c579 | 129 | 41,516 | 209 | 729 | 1,058 | 1,554 | 1,842 | 0 | 7 |
| **pooled** | **1,341** | | **391** | **1,314** | **1,927** | **6,253** | | **29** | |

Crate graph (`cargo metadata --no-deps --offline`, **working trees**: Fuel HEAD 627c2bb7c and auth-framework HEAD 881c696d are not
`origin/main`, so those two are indicative only; the others have HEAD == origin/main): Fuel 45 crates, 166 normal+build edges, `fuel-ir`
fan-in 27; Unpopped 4/5; PersistAnt 9/18; configurant 6/7; httpant 6/9; **auth-framework, releaseant, coderipper are one crate each**
(KISS has no root `Cargo.toml`, not measured). Edges to a crate less stable by margin ≥0.2 / 0.3 / 0.5: **Fuel 1 / 0 / 0, every other
repo 0 / 0 / 0.**

**Consequence (MEASURED):** crate-level ARC-005 almost never fires on this corpus and is blind on single-crate repos; a module-level
instability reading (§3.3) is needed for ARC-005 to say anything about auth-framework (380 files, one crate).

Not yet measured, to be measured in the rule's own PR: module graphs, per-type member counts, fan-in/out, pass-through shapes, chain
lengths. Fuel's lane is hand-measuring structure (its rows 136-139); its numbers are the ground truth the checks are compared with.

### 2.1 Ground truth from the Fuel lane (RELAYED by the PM, not measured here)

Fuel lane, row 136, `fuel#574` (`docs/design-notes/2026-11-01-structure-measurement.md`, `scripts/structure_report.py`), measured at fuel
`origin/main` 3a41200c. Its stated limits: path-prefix heuristic, no re-export or macro resolution, `cfg(test)` in `src` not separated.

- **Crate level is sound:** 45 members, 166 normal edges, 1 layer violation (`fuel-onnx` -> `fuel`, allowlisted), 0 up-tier source
  references across 5 candidate seams. Its tier file (`fuel-ir/tests/crate_dependency_tiers.txt`) is stale: it names a dissolved
  `fuel-core` and leaves 4 members unplaced. (This agrees with §2: a crate-graph reading of ARC-005 passes Fuel.)
- **The real problem is backend knowledge inside generic crates:** 375 `cfg(feature = <backend>)` lines and 770 `BackendId` /
  `DeviceLocation::X` uses in 11 generic crates; `fuel-dispatch` holds 233 of 594 (`pipelined.rs` 112, `plan.rs` 106,
  `jit_ingest.rs` 91), `fuel-tensor` 58 of 60.
- **Co-change** (255 non-sweep commits, 8 weeks, Jaccard, sweeps touching >12 crates excluded): executor and backends 18% joint
  (independent); `kernels-source` and `vulkan-kernels` J 0.88 (one unit in practice). **Public surface:** executor/backends 874
  sites over 551 distinct items (widest); models/nn/training vs core 3,729 sites but only 119 distinct items.

**What this changes in the design:**

1. These findings are ARC-001/ARC-003/ARC-004 territory (volatile detail inside policy) and MOD-003/MOD-005 territory, and **no
   crate-graph reading sees them**. The P-class checks need *item-level and cfg-level facts*, which is a reason to build the
   `syn` fact layer once (MOD-005 PR) and let later rules read it.
2. New P-class measurement under ARC-003/ARC-004, usable **without** a declaration: per crate, the count of `cfg(feature = "...")`
   lines and of paths naming a variant of a configurable "volatile" enum. The volatile set is a declaration in the A-class form
   (`volatile_names`, §4) and, for an undeclared repo, the Info-level inventory lists the top crates by count so a person can
   declare. Fuel's numbers above are the first validation target: the check must put `fuel-dispatch` and `fuel-tensor` at the top.
3. **Staleness of a declaration is itself a finding** (Fuel's tier file names a dissolved crate and leaves four members
   unplaced): a declared component that names nothing in the workspace, or a workspace member in no component, is reported.
4. Co-change (J) is H-class (MOD-006/ARC-009) and stays advisory; Fuel's numbers are the calibration for it.
5. The census distinction (sites vs distinct items) is adopted for ARC-006 and MOD-001: report **both** counts, because 3,729
   sites over 119 items is a narrow surface and 874 over 551 is a wide one.

## 3. P-class rules, in build order (one small PR each)

Order is value over ease, as the PM set it. Every threshold below is a **starting proposal to be replaced by the measured corpus
percentile in that rule's PR**; none is final.

### 3.1 MOD-005 size and coupling outliers

- **Measures:** per file and per module: non-test code lines (from `syn` spans, minus `#[cfg(test)]` items); per type: member count
  (fields + methods across its `impl`s); fan-out (distinct crate-internal modules a module names in `use`/paths); fan-in (distinct
  other modules naming it).
- **Source:** `syn`. Growth trend needs history and is out of scope (H).
- **Threshold:** a percentile of the *pooled corpus*, with the repo's own percentile shown for context. Why pooled, not per-repo:
  Fuel's own p99 is 11,903 lines, so a self-relative rule would call a 5,000-line file normal in the repo that most needs the finding,
  and in small repos p99 is only ~1,400. Proposal: **Info above pooled p95 (≈1,900 lines)**, **Low above pooled p99 (≈6,250)**; the
  same percentile approach for member count and fan-out, pooled again from the PR's measurement. Size alone is a cue, never a verdict
  (`mod.md`): the text says "inspect", and the Low band requires size **and** top-decile fan-in or fan-out.
- **[ASK 1]** pooled-corpus percentile vs per-repo percentile vs a fixed floor; the measured counts above support pooled.
- **Expected volume (MEASURED, rough):** about 5% of pooled files (~67) exceed p95; Fuel contributes most.

### 3.2 MOD-002 cycles between modules

- **Measures:** strongly connected components (Tarjan) of the module graph (`mod` tree, plus `use crate::`, `use super::`,
  `use self::`, and path expressions resolved syntactically). Reports SCC size, the member modules, and the **fewest edges whose
  removal makes it acyclic** (feedback-edge set by greedy back-edge from a DFS rooted at the crate root; labelled "a cut", not
  "the cut").
- **Source:** `syn`. Under-approximates (§1).
- **Decision needed [ASK 2]:** a child module naming its parent's items (`use super::X`) is an edge child→parent while the parent
  owns the child. Proposal: **ignore descendant→ancestor edges** (containment, idiomatic Rust) and flag only cross-branch edges and
  ancestor→descendant items that the descendant also reaches back to by a non-`super` path. Without this the rule fires on most of
  Rust. Cycles *between crates* cannot occur in cargo; that part is ARC-002.
- **Severity:** Low for a cycle across top-level modules; Info for a cycle inside one top-level module.

### 3.3 ARC-005 depend toward stability

- **Measures:** instability `I = Ce / (Ca + Ce)` per component; an edge A→B is flagged when `I(B) - I(A) ≥ margin`, ranked by margin.
  Components are workspace crates (from `cargo metadata`; normal and build edges, optional deps counted because the knowledge base
  says feature-conditional edges are real) **and**, for single-crate repos, top-level modules (from MOD-002's graph).
- **Threshold:** margin 0.3 as a start. MEASURED at crate level: 0 flagged at 0.3 on the whole corpus, 1 at 0.2 (Fuel). So the
  crate-level half will mostly report nothing, which is an honest result, and the module-level half is where findings will come from
  (not yet measured).
- **Caveat stated in text:** Martin's abstractness is not used (weak signal in Rust, per `arc.md`).

### 3.4 MOD-003 reaching into internals

- **Measures:** a `use`/path into a module's non-`pub` descendant is a compiler error across crates, so within a crate the
  question is "reaches past a module's entry point". Needs each module's public interface: a module's interface is its
  `pub`/`pub(crate)` items and its `mod.rs`/parent `pub use` re-exports; a path that names a *grandchild* module of a sibling
  (`crate::a::b::c::X` from outside `a`) is a reach-through unless `a` re-exports `X`.
- **[ASK 3]** depth rule: flag at path depth ≥ 3 below the sibling's root, or only where a `pub use` re-export exists and was bypassed? The second is
  fewer false positives, the first more findings. Proposal: the second.

### 3.5 MOD-004 types that split into usage clusters

- **Measures:** per type, a bipartite graph of fields ↔ methods (method touches `self.f`); connected components over methods
  that share a field; report when ≥ 2 components each have ≥ 2 methods and ≥ 1 field. Low confidence alone (`mod.md`): Info, and the
  text names the clusters so a human judges.
- **Source:** `syn` (field accesses through `self`). Delegation through `self.helper()` calls links methods (an edge), so a method
  call within the type merges clusters.

### 3.6 MOD-008 pass-through wrappers

- **Measures:** a fn/method whose body is a single call that forwards its parameters unchanged (same order, no transformation, no
  added argument other than `self.field`), with the same or a trivially wrapped return. Reports the pair (wrapper, callee).
- **Exceptions that are not findings:** trait-impl methods (the signature is dictated), `#[inline]` single-hop adapters named in a
  `From`/`Deref`/`AsRef` impl, and `pub` items in a crate's top-level module (a facade). These are rules about *kind of item*, not path
  exclusions. A model confirms "adds nothing"; the check reports the shape (V1).
- **Measure first:** how many pass-throughs exist in the corpus and how many are facades.

### 3.7 MOD-009 long reach-through chains

- **Measures:** field-access/method-call chains `a.b().c.d()` of length > N where intermediate links are not iterator/builder
  adapters (`iter`, `map`, `filter`, `collect`, `clone`, `unwrap*`, `as_*`, `into*`, `?`, `await`, builder methods returning `Self`).
  N configurable, proposal 4 pending the measured distribution. Low precision, Info (`mod.md`).

### 3.8 MOD-010 invariant-bearing types keep fields private

- **Needs a tag** (A-flavoured): a type is invariant-bearing if declared so (§4) or if it has a validating constructor
  (`new`/`try_new`/`from_*` returning `Result`/`Option` that contains a check) *and* a public field. The inferred half is a
  heuristic and reported as Info; the declared half as Low.

### 3.9 MOD-001 finish: public surface

Fold in E8 `reachability` precision (evidence: `.lane-state/e8-reachability-evidence.md`: Fuel 497 Medium rows, auth-framework 637;
classes (a) type reached via field/return/literal/variant, (b) pub API of a library crate root is a root, (c) test-only callers,
(d) name collisions, (e) non-fn items, (f) dead-root chains). Add: a **pub item count** per crate, `pub` items with no doc comment, and
`unreachable_pub` as a delegated rustc lint (the triage names it) rather than a rebuild. Documented `pub` of a library's root is a
root, not dead (Fuel's and auth-framework's callers are consumers).

### 3.10 ARC-004, ARC-008, ARC-012 (P, last of the P group)

- **ARC-004** infrastructure types in policy signatures: needs a *forbidden namespace list* (a declared list of crates/paths, e.g.
  `sqlx::`, `reqwest::`, `axum::`) and a *policy component* tag, so it is A-flavoured in practice. Syntactic signature scan; no type
  resolution until a rustc-based fact source exists. Stated limitation: a type alias hides it.
- **ARC-008** indirection that earns nothing: shapes only (a trait with one impl and no test double; a `*Service` → `*Repository` →
  `*Adapter` chain of single-forwarding types). The "earns nothing" judgment is not made by the check; it reports the shape.
- **ARC-012** construction only in composition roots: needs declared roots (§4); flags `Type::new(...)`/struct literals of declared
  "wired" types outside a root.

## 4. A-class: the declaration format (ARC-001/002/003/006/007)

Declared in `.coderipper.toml` under `[architecture]`, read through the existing configuration loader (so system/user files and
`--no-config` behave as for every rule). A project with no `[architecture]` gets **no A-class findings and a coverage line
saying they did not run** (never "clean"). Proposed shape:

```toml
[architecture]
style = "layered"          # informational only; every check derives from components and edges

[[architecture.component]]
name   = "domain"
paths  = ["crates/fuel-ir"]            # crates or module paths (workspace-relative)
role   = "policy"                      # policy | adapter | infrastructure | composition
volatility = "stable"                  # stable | volatile  (tag used by ARC-003 and ARC-005 context)
volatile_names = ["BackendId", "DeviceLocation"]   # enums/paths whose variants are volatile detail (ARC-003/004)
invariant_types = ["Money", "Shape"]   # MOD-010
owns_data = ["orders"]                 # ARC-007

[[architecture.allow]]                 # an edge between components that is permitted
from = "adapter-http"
to   = "domain"

[architecture.composition_roots]       # ARC-012
roots = ["src/main.rs"]

[architecture.api_budget]              # ARC-006
domain = 40                            # max pub items

[[architecture.waiver]]                # a deliberate exception, never a silent one
edge   = ["domain", "adapter-sql"]
reason = "…"
owner  = "…"
until  = "2027-03-01"
```

Rule use: ARC-001 compares the real graph with `allow`; ARC-002 flags cycles *between* components (inside one: Info);
ARC-003 flags a `stable`/`policy` component depending on a `volatile`/`infrastructure` one (feature-conditional edges counted);
ARC-006 counts `pub` items per component against the budget and flags re-exported third-party types; ARC-007 flags a second
component naming a table/collection in `owns_data`. All declared-vs-real checks are V0 (exact for the facts they see).

**Drafting a starting declaration** (so adoption is not blocked): a later `coderipper architecture draft` subcommand partitions the
crate/module graph and writes a *proposal file with confidence marks*, never active until a person copies it into
`[architecture]` (V5). Out of scope for the first A-class PR; named here so the format leaves room (`confidence` fields).

**[ASK 4]** the format above is a proposal. Choices that are the owner's: TOML keys and names; crate-granular vs path-granular
components; whether `volatility` is a closed set or free tags; where a repo's declaration lives (own `.coderipper.toml`, or a
separate `architecture.toml` the knowledge base calls its own file).

## 5. H and L rules (advisory, last)

ARC-009, ARC-011, MOD-006 need history (co-change from `git log`) or a service graph; ARC-010 and MOD-007 need a model. They run
at advisory severity and are scheduled after the A-class PRs. After ARC and MOD, a table of the remaining ~190 rules (class,
feasibility, effort, tool) is delivered so they can be scheduled.

## 6. Build order and PR plan

1. This note (docs only). 2. P-class first, one PR per rule, ordered by the owner's stated interest (separation of concerns, modularity, encapsulation,
abstraction, SRP, loose coupling, high cohesion; CireSnave 2026-11-01): MOD-005 (builds the shared fact layer), MOD-004 (cohesion/SRP),
MOD-003 and MOD-002 (modularity), MOD-010 and MOD-001 with E8 (encapsulation), MOD-008 and ARC-008 (abstraction), ARC-005 and MOD-009
(coupling), ARC-004, then ARC-012. 3. A-class on §4. 4. H/L advisory. 5. The remaining-rules table.
Each PR: fixtures both ways, mutation check, measured FP counts on the corpus, catalogue row updated, CHANGELOG, version proposal.

## 7. Open choices for the PM ([ASK])

1. MOD-005 threshold reference (pooled / per-repo / floor). 2. MOD-002 treatment of child→parent edges. 3. MOD-003 depth rule.
4. The `[architecture]` declaration format. 5. Whether the module-level ARC-005 reading ships with the crate-level reading or after.

## 8. What this note does not do

No check is built, no catalogue row changed, no threshold is final. The table in §2 counts whole-file lines
(inline `#[cfg(test)]` included); the MOD-005 PR replaces it with non-test lines from `syn`.
