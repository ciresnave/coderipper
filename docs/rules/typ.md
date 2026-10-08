Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

# Letting the compiler guard the domain (TYP)

## Why it matters

Many defects are states the program can express but should never reach: text where an enumeration belongs, a float
holding a price, a field that may be null although it never should be, or checks that live in several places until one
is forgotten. Moving those guarantees into types shifts the work from runtime checks and reviewers to the compiler.
Evidence for this is moderate and growing, and it is strongest when a type captures a real invariant. The aim, often
phrased as "make illegal states unrepresentable", is that the bad state cannot even be written down.

## How to apply it

**Handle outside data once, at the edge**
- Convert untrusted input where it arrives into domain types whose fields are private and whose constructors validate.
  Everything behind that edge works with values that are valid by construction and need no second look.
- Pick the two or three concepts the code gets wrong most often (user ids, prices, timestamps) and start there. Put one
  parsing function at each entry point and make it the only place that builds the type from raw data.

**Choose types that carry meaning**
- Model states and alternatives with sum types (enums, unions), not strings or flags beside nullable fields. Replace
  stringly-typed states first and let the compiler list every place that now needs a decision.
- Give identifiers, units and currencies their own types. Hold money as whole numbers of the smallest unit, or in a
  decimal type, never as binary floating point.

**Make absence and order explicit**
- Use an optional type only where a value may truly be missing, and keep `unwrap`, `!!` and unchecked `.get()` off
  production paths.
- When operations must happen in sequence (open, then configure, then run), encode the sequence in types (typestate) so
  an out-of-order call does not compile.
- Let an aggregate change only through one entry point that guards its invariants. Where the type cannot express an
  invariant, write it next to the type and back it with an assertion or a property-based test.

**Tags**
- Several rules (TYP-003, TYP-005) only make sense for types or modules the team has marked as carrying an invariant or
  handling money. The mark can be an annotation or an entry in the project's declared configuration. Without it such
  checks would stay quiet rather than guess.

## Background and lineage

Hoare called the null reference his billion-dollar mistake. Minsky's slogan for OCaml, to make illegal states
unrepresentable, states the goal directly. Strom and Yemini introduced typestate in the 1980s, Evans described value
objects, entities and aggregates, and the Haskell tradition supplied smart constructors and newtypes. King's "Parse,
don't validate" turns the goal into a habit: make the edge produce a type that is valid by construction, one time. Meyer's design by
contract covers what types alone cannot express.

## Measures and numbers

This domain has no numeric thresholds. Its checks look for presence and shape: public signatures built from bare
primitives, string comparisons against fixed sets, floats in money-tagged modules, unwrap-style calls on production
paths, and booleans in signatures.

## Cautions

- Avoid a flood of newtypes and conversion code for values with no invariant and no realistic risk of mix-up. Reserve
  the technique for places with a real invariant or a believable chance of mixing values up.
- In dynamic languages the same discipline is carried out at the edge through libraries that validate and check types at
  checks, for example zod or pydantic.
- Strict parsing at boundaries is preferred to being liberal in what is accepted; any tolerance should be explicit,
  documented and bounded. This is one of the points on which schools disagree.
- Name-based heuristics for domain primitives (TYP-001) and for money (TYP-005) are low precision; they need module tags
  or an LLM pass to be reliable.

## Where it is checked

None of these rules is run by the current release. The notes say what is planned or why nothing can run yet.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| TYP-001 | Public APIs wrap domain values in dedicated types | P | no check yet; planned: native name heuristics; semantic judgement needs an LLM |
| TYP-002 | Strings standing in for enums | P | no check yet; planned: native syntax-tree check |
| TYP-003 | Invariant-bearing types can only be built through validation | P | no check yet; planned: native visibility check, which needs a declared tag or annotation |
| TYP-004 | Outside data becomes domain types at the boundary | P | no check yet; planned: native flow check for TypeScript and Python; Rust needs data-flow analysis |
| TYP-005 | Money and exact amounts avoid binary floats | P | no check yet; planned: native check inside money-tagged modules, which needs a declared architecture of tagged modules |
| TYP-006 | No unwrap or null assertions on production paths | P | no check yet; planned: surface Clippy unwrap_used, typescript-eslint no-non-null-assertion and Pyright |
| TYP-007 | Boolean blindness in signatures | P | no check yet; planned: Clippy and Ruff results; no TypeScript tool (licence) |
| TYP-008 | Invariants are written down and checked | P | no check yet; planned: native presence check of docs versus enforcement; consistency needs an LLM |
| TYP-009 | State machines make transitions explicit | P | no check yet; planned: native check for TypeScript and Python; Rust needs a model or tests |

## Further reading

- "Null References: The Billion Dollar Mistake", Tony Hoare
- "Typestate: A Programming Language Concept for Enhancing Software Reliability", Robert Strom and Shaula Yemini
- *Domain-Driven Design*, Eric Evans
- "Parse, don't validate", Alexis King
- *Object-Oriented Software Construction* (design by contract), Bertrand Meyer
- Yaron Minsky's writing on making illegal states unrepresentable in OCaml
