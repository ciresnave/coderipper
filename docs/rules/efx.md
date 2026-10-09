Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

# Keeping effects at the edge (EFX)

## Why it matters

When state and side effects hide inside logic, a function's behaviour depends on things its signature does not show.
The usual sources are the system clock, random numbers, environment settings, disk access, network calls and global
mutable singletons that creep into business rules. Such code is hard to reason about locally, hard to test and
sometimes non-deterministic. The remedy is to keep effects at the edge of the program and leave the middle as plain
computation over values. The support is practical experience with testability rather than controlled studies.

## How to apply it

**Separate computation from effects**
- Business computation consumes plain values and yields plain values. Reading files, asking the time, generating random
  numbers and touching the environment all happen in a thin outer layer.
- Start by listing the effects your domain code performs today (time, randomness, environment, disk, network, threads),
  add a small parameter or interface at the edge for each, and move the real call into the outer layer.
- Mark the modules that are meant to be pure so a deny-list of effectful imports can protect them.

**Make nondeterminism something you pass in**
- Give code a clock, a random source or a filesystem handle as a parameter, so a test can substitute a controlled one.
- Gather environment reads into one configuration module that produces a typed settings value at start-up.

**Limit shared state**
- Prefer immutable data. Mutating a local variable inside a function is harmless; mutation that is shared needs a
  stated reason.
- Allow global mutable state only in composition roots and a short allowlist (logging, metrics). A singleton is a
  global with a nicer name. Answer a global-state finding by passing the state in, or by allowlisting the module with a
  reason.
- Where interior mutability or a cache is needed, say who owns it and who may change it.

**Keep object APIs honest**
- Separate commands from queries: a method either changes state or reports something. Sensible exceptions exist, for
  example removing and returning the top of a stack, or an atomic compare-and-swap.
- For algorithms that must be repeatable, make the same input give the same output regardless of thread count or order,
  and add a test that runs them twice and compares a hash of the result.

## Background and lineage

Referential transparency, the property that an expression can be replaced by its value, anchors functional
programming, and Haskell's IO monad showed how to keep effects visible in types. Meyer's Command-Query Separation,
Ports and Adapters, Bernhardt's functional core with an imperative shell, and the Elm architecture are practical
descendants. Immutable data structures help, and Rust's ownership rules act as a purity mechanism by limiting shared
mutation at compile time. Passing a clock, random source or filesystem in as an explicit capability finishes the job.

## Measures and numbers

No numeric thresholds apply. The purity rules rely on a declaration: either a tag on the component or a documentation
claim. Where nothing is declared, a check has nothing to hold the code to.

## Cautions

- Dogmatic purity backfires: forcing state-monad plumbing onto code that is obviously stateful adds ceremony with no
  payoff.
- Do not inject capabilities for values that never change.
- Command-query mixing (EFX-005) is a low-precision heuristic and is advisory only.

## Where it is checked

None of these rules is run by the current release. The notes say what is planned or why nothing can run yet.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| EFX-001 | Components declared pure avoid effectful APIs | A | no check yet; needs a declared architecture (a deny-list per tagged component) |
| EFX-002 | Global mutable state only where allowed | P | no check yet; planned: native scan in Rust, TypeScript and Python |
| EFX-003 | Environment reads are centralized | N | no check yet; planned: native regex for common environment reads found anywhere but the config module |
| EFX-004 | Deterministic algorithms give identical results every run | X | no check yet; needs running the algorithm repeatedly and comparing hashes |
| EFX-005 | Methods that both change state and answer queries | P | no check yet; planned: native heuristic, advisory |
| EFX-006 | Functions claimed pure are pure | P | no check yet; planned: native call-graph check for TypeScript and Python; Rust needs effect reachability |
| EFX-007 | Shared-mutation spots say who owns them | P | no check yet; planned: native check that cells, locks and caches carry an ownership comment |

## Further reading

- *Object-Oriented Software Construction* (Command-Query Separation), Bertrand Meyer
- "Boundaries" (functional core, imperative shell), Gary Bernhardt
- Ports and Adapters (hexagonal architecture), Alistair Cockburn
- The Elm architecture
- Documentation on Rust ownership and borrowing
