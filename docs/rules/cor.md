# COR: being right, not just tidy

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

A codebase can be neatly modular, simple and well layered, and still compute the wrong answer. Tests only
sample behavior. To know a program is right you also need written-down expectations: invariants,
preconditions, postconditions, permitted state transitions, guarantees about ordering and repeated
application, and numeric tolerances. This was the largest gap in the first edition of the knowledge base. The
evidence is strong (E) that contracts and generated-input tests uncover faults which hand-picked examples
overlook, and weaker (C) when it comes to the cost and benefit of heavy formal methods.

## How to apply

**Write expectations where they will be enforced.**
- Begin with types; next come assertions at the edges of modules (`debug_assert`, contract checks); after those, tests, and finally
  prose. API docs name the preconditions, the error behavior, the panics and the safety obligations.
- Keep `unsafe` rare. Forbid it by default; where it stays, keep the scope small, give a `SAFETY` rationale
  and cover it with Miri or a sanitizer.
- Treat numbers with care: declare tolerances, never compare floats for equality, set a policy for NaN and
  infinity, and decide whether results must match across platforms.

**Test properties and compare against oracles.**
- Use property tests for algebraic laws: round trips between encode and decode, idempotence, commutativity,
  monotonicity, conserved totals, stable ordering.
- Differential testing checks a fast implementation against a trusted or deliberately simple one. Where no
  oracle exists, metamorphic testing transforms the input and predicts how the output must relate. Both
  are most valuable when a component is being rewritten.

**Go further only where the risk justifies it.**
- Model the protocols that matter most, such as concurrent or distributed ones, using TLA+ or Alloy, and link
  the code to the model.
- Prove small critical kernels, including `unsafe` ones, using bounded model checkers or deductive verifiers.

## Background and lineage

The lineage runs from Floyd-Hoare logic and Dijkstra's weakest preconditions, through Meyer's design by
contract in Eiffel, to assertions and specification languages such as JML. Property-based testing arrived
with QuickCheck and spread to Hypothesis, proptest, jqwik and fast-check. For models and proofs there are
TLA+ and Alloy, refinement types, and verifiers such as Dafny and, for Rust, Kani, Prusti, Creusot and
Verus. Miri and the sanitizers detect undefined behavior at run time, and differential and metamorphic
testing cover cases without an easy oracle.

## Measures and numbers

No numeric thresholds apply. Rigor is tiered by component criticality as declared in the project's
specification.

## Cautions

Formal methods applied everywhere cost more than they return, and assertions that repeat what the type
system already guarantees add noise. Scale rigor to risk, and let the specification tier components by
criticality.  The cost of formal modelling and proof is the reason these two rules are low severity and low confidence.

## Where it is checked

Only five checks exist today (none for this domain), so no rule below is run by the current release. Every row is a catalog record with no check yet; "planned" names the tool or native check the triage proposes.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| COR-001 | Public API docs state errors, panics and safety | P | catalog record; no check yet. Planned: Clippy `missing_errors_doc`, `missing_panics_doc`, `missing_safety_doc` (Rust), `jsdoc/require-throws` (TypeScript), Ruff `DOC501` and `DOC201` (Python, preview) |
| COR-002 | Invariant types enforce invariants in code | P | catalog record; no check yet. Planned: native constructor-validation check in all three languages; comparing it with the documented invariant needs an LLM |
| COR-003 | Codec pairs have round-trip property tests | P | catalog record; no check yet. Planned: native pair detection plus property-test presence for TypeScript and Python; Rust detection not designed yet |
| COR-004 | Reference-implementation algorithms get differential tests | X | catalog record; no check yet, needs a spec tag and a test harness |
| COR-005 | Requirements and doc claims traced to asserting tests | L | catalog record; no check yet, needs an LLM to judge whether assertions imply the claim |
| COR-006 | Unsafe confined, justified and run under Miri | P | catalog record; no check yet. Planned: Miri plus a `forbid(unsafe_code)` and SAFETY inventory (Rust); not applicable to TypeScript and Python |
| COR-007 | No float equality; tolerances declared | P | catalog record; no check yet. Planned: Clippy `float_cmp` (Rust), native AST checks for TypeScript and Python |
| COR-008 | Critical protocols modelled and code traced to model | A | catalog record; no check yet, needs a declared specification and model artifacts |
| COR-009 | Critical functions have proof harnesses in CI | X | catalog record; no check yet, which functions are critical comes from the spec |
| COR-010 | Idempotency and ordering claims tested with replay | X | catalog record; no check yet, needs a fault and replay harness |

## Further reading

- C. A. R. Hoare, "An Axiomatic Basis for Computer Programming"
- Edsger Dijkstra, *A Discipline of Programming*
- Bertrand Meyer, *Object-Oriented Software Construction*
- Koen Claessen and John Hughes, "QuickCheck: A Lightweight Tool for Random Testing of Haskell Programs"
- Leslie Lamport, *Specifying Systems*
- Daniel Jackson, *Software Abstractions*
