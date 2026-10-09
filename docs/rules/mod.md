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
| MOD-001 | Small, documented public surface | P | catalog record; partial overlap with legacy check `reachability`; no other check yet (triage names rustc `unreachable_pub` and `missing_docs`, api-extractor for TypeScript, an `__all__` audit for Python) |
| MOD-002 | No dependency cycles between modules | P | catalog record; no check yet (triage names cargo-modules, dependency-cruiser and pylint for the three languages) |
| MOD-003 | No reaching into a module's internals | P | catalog record; no check yet, needs a notion of each module's public interface (triage names eslint-plugin-import and import-linter; Rust privacy is enforced by the compiler across crates) |
| MOD-004 | Types that split into separate usage clusters | P | catalog record; no check yet (planned native fields-by-method graph; low confidence alone) |
| MOD-005 | Size and coupling outliers within the codebase | P | catalog record; no check yet (planned native percentile facts; growth trend needs history) |
| MOD-006 | Divergent change across unrelated edits | H | catalog record; no check yet, needs history mining |
| MOD-007 | Stated responsibility compared with declared purpose | L | catalog record; no check yet, needs a human or model judgment; advisory |
| MOD-008 | Pass-through wrappers | P | catalog record; no check yet (planned native AST pattern; a model confirms nothing is added) |
| MOD-009 | Long reach-through chains | P | catalog record; no check yet (planned native chain-length check; low precision) |
| MOD-010 | Invariant-bearing types keep fields private | P | catalog record; no check yet, needs the invariant-bearing tag |

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
