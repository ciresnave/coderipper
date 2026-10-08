Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

# Code that is easy to read and name (RDB)

## Why it matters

Code is read many times for each time it is written; a ratio of about ten to one is often cited. A reader can keep
only so much in mind at once, so how readable a unit is decides how costly it is to change and how many mistakes get
in. The practical conclusion is to manage readability as a burden on the reader, which can be measured and reduced,
rather than as a question of taste. Support for this is moderate for cognitive complexity as a predictor of
comprehension effort, reasonable for advice on naming and structure, and weak for rigid size limits.

## How to apply it

**Shape the code so it can be understood locally**
- A reader should grasp a unit without hunting through many files. Chopping code into tiny fragments can make that
  harder, not easier.
- Exit early on the failure cases, keep nesting shallow, and pull out pieces only when they deserve a name from the
  problem domain, not at arbitrary line counts.
- Keep functions at one level of abstraction, and avoid hidden coupling between them.

**Name things well**
- Names should show intent in the vocabulary of the domain, with one term per concept throughout the codebase.
- Put units in the name or the type, avoid cryptic abbreviations and encoded prefixes, and let name length grow with
  scope.
- Follow the idiom of the language in use (PEP 8, the Rust API guidelines, the Java code conventions). Agreement across
  the codebase matters more than anyone's preference, so keep a glossary and settle on a single approach for each
  recurring task.
- Let a formatter and linter decide layout so nobody argues about it.

**Write comments that carry reasons**
- Record constraints, invariants and decisions, and link to design records.
- Do not narrate what the code already says, do not leave code commented out, and give every TODO an owner and an
  expiry date.

**Remove duplication of knowledge, not of text**
- One fact should live in one place. Lines that look alike may express different ideas, and two constants that merely
  hold equal values do not form one idea.
- Wait for a third occurrence before abstracting, and prefer a little duplication to the wrong abstraction.

**Treat smells as prompts**
- Unreachable code, unexplained literals, flag arguments and over-long argument lists are familiar smells that the
  field regards as well understood. Use them as a reason to look closer, not as verdicts.

## Background and lineage

The ideas build on Kernighan and Plauger's style advice, McCabe's cyclomatic complexity (a good way to count test
paths, a weak guide to how hard code is to understand), Knuth's literate programming, McConnell's *Code Complete*,
Beck's rules for simple design (four of them), Fowler's refactoring catalogue of smells, and Hunt and Thomas on DRY. The newer
cognitive-complexity measure came from SonarSource. Hickey's *Simple Made Easy* separates simple (not entangled) from
familiar. Pike's maxim that data dominates points the same way, and Knuth's remark that most small efficiencies do not
matter, while a few critical ones do, warns against optimising prematurely at the cost of clarity. Ousterhout's work
on deep modules supplies the counterweight to over-splitting.

## Measures and numbers

- Cognitive complexity per function: a sensible default is to warn at 15 and fail at 30, and to gate on regression, so
  that a function being edited does not get worse. Generated code, parsers and state-machine implementations can be
  exempted, with the reason written down.
- No default function-length limit is proposed; complexity, nesting and local readability are preferred, and both
  god-functions and pass-through fragments are worth flagging.

| Smell | Typical detector | Tier |
| :--- | :--- | :--- |
| Oversized functions and classes | size and complexity percentiles | V1 |
| Many arguments, flag arguments | counting arguments; spotting boolean ones | V1 |
| Duplicated code | token or syntax-tree clone detection; embeddings or an LLM for semantic clones | V1, V4 |
| Feature envy | a method leans on another type's fields more than on its own | V1 |
| Data clumps, primitive obsession | parameter groups that repeat; domain-named primitives | V1, V4 |
| Repeated switch on one tag | pattern search across functions | V1 |
| Message chains, middle man | call-chain depth; bodies that only delegate | V1 |
| Shotgun surgery, divergent change | files that change together in history | V3 |
| Speculative generality, unreachable code | reference analysis and compiler warnings | V0, V1 |
| Comment problems | out-of-date TODOs at V0, disabled code at V1, drift from behaviour at V4 | V0 to V4 |

## Cautions

- Arbitrary function-length limits are contested between the small-functions school and the deep-modules school; do
  not enforce them as if they were proven.
- Applying DRY to coincidental similarity creates coupling that was not there before.
- Whether comments help at all is argued; the stance taken here is to encourage "why" comments and reject stale or
  restating ones.
- A cognitive-complexity score is evidence for a conversation, not a ruling.
- Magic-literal and name-versus-behaviour checks are advisory because their precision is low.

## Where it is checked

None of these rules is run by the current release, except in part RDB-006 through the legacy checks named below. The
notes say what is planned or why nothing can run yet.

The legacy checks `unused-parameters` and `reachability` are the only existing checks that touch this domain, and they
overlap RDB-006: the first finds unused function parameters, the second finds unreferenced items, including public
ones. Both are narrower than the rule, which also asks for no unused imports or private items.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| RDB-001 | Code matches the project formatter | P | no check yet; planned: run rustfmt, Prettier or Ruff format in check mode |
| RDB-002 | Identifiers follow language naming conventions | P | no check yet; planned: surface rustc naming lints, typescript-eslint naming-convention, Ruff pep8-naming |
| RDB-003 | Cognitive complexity stays within bounds | P | no check yet; planned: Clippy and complexipy results plus a baseline for regression gating; no TypeScript tool (licence) |
| RDB-004 | Nesting, parameter count and flag arguments stay small | P | no check yet; planned: Clippy, ESLint and Ruff limits; nesting depth is not covered in Rust or Python |
| RDB-005 | Duplicated knowledge across clone groups | P | no check yet; planned: jscpd clone groups; churn weighting and semantic clones need history and an LLM |
| RDB-006 | No dead code, unused imports or unused private items | P | no check yet beyond the legacy checks `unused-parameters` and `reachability`, which cover only part of it; planned: rustc, knip, vulture |
| RDB-007 | Magic literals get names | P | no check yet; planned: native literal scan for Rust, ESLint and Ruff elsewhere; advisory |
| RDB-008 | Comment hygiene: owned TODOs, no commented-out code | N | no check yet; planned: native regex for TODO owners, weaker heuristic for commented-out code |
| RDB-009 | Names tell the truth about behaviour | L | no check yet; needs an LLM tier |
| RDB-010 | Comments and docstrings match current behaviour | L | no check yet; needs an LLM tier (a parameter-name cross-check could be deterministic) |
| RDB-011 | Terminology follows the project glossary | N | no check yet; planned: native glossary lookup over identifier tokens |
| RDB-012 | Additions match how the repository already works | L | no check yet; needs an LLM tier |
| RDB-013 | Diff readability against a rubric | L | no check yet; needs an LLM tier; advisory |

## Further reading

- *The Elements of Programming Style*, Brian Kernighan and P. J. Plauger
- *Code Complete*, Steve McConnell
- *Refactoring*, Martin Fowler
- *The Pragmatic Programmer*, Andrew Hunt and David Thomas
- *Simple Made Easy* (talk), Rich Hickey
- John Ousterhout, *A Philosophy of Software Design*
- "Cognitive Complexity: A new way of measuring understandability", G. Ann Campbell (SonarSource)
- "A Complexity Measure", Thomas McCabe
- "Literate Programming", Donald Knuth
