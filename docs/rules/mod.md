# Keeping parts small, focused and closed: the MOD rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Changing a program is expensive mostly because of difficulty nobody needed to have: duties that
are knotted together and dependencies that are not visible. In that situation an edit in one
place forces edits elsewhere, and nobody can say where one concern ends. Parts that each have a
clear job, a small interface and a private interior avoid this. The evidence is largely
practitioner experience, plus measurements showing that tight coupling and churn-heavy code are costly.

## How to apply it

**Deciding where the seams go**

- Cut along reasons for change, not along size or counts of nouns. Martin later sharpened the
  single-responsibility principle to "answers to one actor": keep together what changes together
  for one stakeholder, and separate whatever changes for other stakeholders. The phrase "does one thing" is
  a looser test.
- Judge cohesion by shared state, not by length. A big type whose parts always move in step can
  be sound, whereas a tiny type that answers to two stakeholders is not. Length is a cue to inspect, never a
  verdict.
- Use one line of reasoning at every scale, from a function to a whole service.

**Hiding what should be hidden**

- Make things private by default and export narrowly (`pub(crate)`, package-private, `internal`).
  Visibility is how information hiding is actually enforced.
- Give types that guard an invariant private fields and a construction path that validates.
- Reach other modules only through their entry points.
- Do not dig through objects to touch their internals; ask the nearest collaborator to act (the
  Law of Demeter, or "tell, don't ask"). Iterator pipelines and fluent builders are fine.

**Making modules worth having**

- Prefer deep modules: they take on more hidden difficulty than their interface asks the caller
  to learn. A thin wrapper that merely forwards a call adds surface and hides nothing.
- Compare sizes within the codebase. Use percentiles of size, member count and fan-out, together
  with afferent (incoming) and efferent (outgoing) coupling, rather than fixed limits.

## Background and lineage

Dijkstra proposed separation of concerns and, with the THE system, layered design. Parnas
(1972) argued for carving a system up around the decisions most expected to vary, each
shut behind a module interface. Stevens, Myers and Constantine supplied the cohesion and coupling
taxonomies. McIlroy's Unix philosophy favoured small programs, each good at one job, that can be combined.
Meyer's *Object-Oriented Software Construction*, Evans's bounded contexts, Hickey's distinction
between simple and easy, and Ousterhout's deep modules complete the line.

## Measures and numbers

Percentile rank of size, members and fan-out within the same codebase, plus its growth;
afferent and efferent coupling counts; connected components of the graph linking members to the
state they use (a low-confidence cohesion signal); and the length of reach-through chains, whose
limit is configurable.

## Cautions

The opposite failure is as real. Hundreds of one-method classes, or "services" that only
forward, scatter logic and send readers hopping between files. A set of checks for this domain
has to flag both over-large modules and pass-through confetti. A name containing "And" is no
evidence; at most it prompts a human or model to look. Cohesion scores are weak alone, and the
reach-through and LLM responsibility rules are advisory.

## Where it is checked

The legacy check `reachability` overlaps MOD-001 in part: it finds public items nothing uses,
one slice of a minimal public surface (it also overlaps RDB-006, dead code). The rest of MOD-001
is not covered by it.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| MOD-001 | Small, documented public surface | P | check `MOD-001` (Rust, from the source with `syn`, advisory Info/Low, medium confidence; default on in `extended` and `portfolio`): per library package, the public declarations (reachable through `pub` modules, plus `pub use` leaves) and the public inherent functions of public types that carry no documentation (`///`, `#[doc = ..]`, `#[doc(..)]` other than `hidden`, or the `//!` inside a module's own file), with both counts (items and members of the public types) and the first places. Info for any gap, Low from half of at least 20 items. The size of the surface is stated, not judged (a declared ceiling is `ARC-006`). Not done: the reachability check's precision (E8) and the delegated `unreachable_pub` and `missing_docs` lints. The legacy check `reachability` finds public items nothing uses |
| MOD-002 | No dependency cycles between modules | P | check `MOD-002` (Rust, from the source with `syn`, advisory Low/Info, medium confidence; default on in `extended` and `portfolio`): the strongly connected sets of two or more modules in the package's module graph. A child naming its parent (`super`, `self`) is not an edge, a descendant reaching back by a `crate::` path is, the crate root takes no part and the `use` of a `pub use` is not an edge (the design note's question 2, as it proposed). Low when the set spans two or more top-level modules, Info inside one. One finding per set with the edges (file and line each) and the edges whose removal dissolves it. A lower bound: macro tokens, aliases and glob re-exports are not followed |
| MOD-003 | No reaching into a module's internals | P | check `MOD-003` (Rust, from the source with `syn`, advisory Low, medium confidence; default on in `extended` and `portfolio`): a path (or `use`) that goes through at least one inner module of a sibling to an item that a module on the way re-exports with `pub use`, `pub(crate) use` or `pub(super) use` (by name, renamed, or by a glob of the module that holds it). One finding per file with the count, the first line and the shorter path the owner offers. A reach with no re-export in the way is not reported (nothing was bypassed). Syntactic: a re-export through a macro or alias is not seen, a path in a macro call's tokens is not read, a glob re-export is taken to offer every item |
| MOD-004 | Types that split into separate usage clusters | P | check `MOD-004` (Rust, `syn`, advisory, low confidence): methods grouped by the `self` fields they use, directly or through the siblings they call; accessors, derived values and builder steps are not members; a field reached by destructuring is not seen. **Known limit: recall is low** (6 of 3,706 structs in our corpus): Fuel's `Tensor` (252 methods) and `VulkanBackend` (183) are not reported, because their methods reach one another through shared state or helpers and so form one cluster; those two are the next fixtures |
| MOD-005 | Size and coupling outliers within the codebase | P | check `MOD-005` (Rust, `syn`, advisory): files by code lines, fan-out and fan-in, types by fields plus inherent methods, against percentiles of a pooled corpus of our own codebases; growth over time needs history and is not covered; macro-generated items, glob-import targets and `#[path]` modules are not seen |
| MOD-006 | Divergent change across unrelated edits | H | catalog record; no check yet, needs history mining |
| MOD-007 | Stated responsibility compared with declared purpose | L | catalog record; no check yet, needs a human or model judgment; advisory |
| MOD-008 | Pass-through wrappers | P | check `MOD-008` (Rust, from the source with `syn`, advisory Info at low confidence; default on in `extended` and `portfolio`): a free function or an inherent method whose whole body is one call that forwards all its parameters, in order, unchanged (`g(a, b)`, `self.g(a)` as an alias, `g(&self.x, a)`), possibly inside `Ok(..)`, `Some(..)`, `?`, `.await` or `.into()`. Not wrappers: a call on a field (`self.inner.go(v)`: it hides the field), a constructor (`Self(x)`, `Self::Variant(x)`), a function with no parameter to forward, a trait impl method, a `pub` function in `lib.rs`/`main.rs` (a facade). The shape only: the finding names the pair and the reasons a layer may be worth keeping, and offers no change |
| MOD-009 | Long reach-through chains | P | catalog record; no check yet (planned native chain-length check; low precision) |
| MOD-010 | Invariant-bearing types keep fields private | A | check `MOD-010` (Rust, from the source with `syn`, advisory; default on in `extended` and `portfolio`): **declared half** (a type named in a component's `invariant_types`, found by name in that component's crates): Low when it has a `pub` field; Info when it derives `Default` or `Deserialize` (without `#[serde(try_from)]`/`from`) while it has a validating-looking constructor; Info when the name is a struct in none of the component's crates. **Inferred half, opt-in** (`[mod010] inferred = true`): Info for a `pub` struct with a validating-looking constructor (`new`, `try_new`, `parse`, `from_*`, at least one argument, returning `Result<Self, _>` or `Option<Self>` and rejecting with `Err`, `None`, `bail!` or `ensure!`) and a `pub` field. Text only: it names the fields and offers no change |

Classes: P is a parser-level fact check, H needs history, L needs a language model.

## Further reading

- Frederick Brooks, "No Silver Bullet"
- E. W. Dijkstra, work on separation of concerns and the THE system
- David Parnas, his 1972 paper on decomposing systems into modules
- Stevens, Myers and Constantine, "Structured Design"
- Doug McIlroy, writings behind the Unix philosophy
- Bertrand Meyer, *Object-Oriented Software Construction*
- Robert C. Martin, writings on the Single Responsibility Principle
- Eric Evans, *Domain-Driven Design*
- Rich Hickey, "Simple Made Easy"
- John Ousterhout, *A Philosophy of Software Design*
