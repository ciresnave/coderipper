# Pointing dependencies the right way: the ARC rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

If nothing limits who may depend on whom, everything ends up depending on everything. The parts
that change most, such as frameworks, databases, transports and vendor SDKs, creep into the code
that holds the business rules, and the price of each change grows faster than the system itself.
Empirical work (MacCormack, Rusnak and Baldwin, 2006) shows that dependency structure can be
measured and differs systematically between designs; the knowledge base holds that this structure
predicts how far a change propagates. Claims about particular architectural styles rest
on practitioner consensus rather than measurement. The opposite error also exists: building
structure too early, so that extra layers and translation steps add cost and no benefit.

## How to apply it

**Declare and enforce the structure**

- Write the architecture down as data a tool can read. The clean, hexagonal, onion, layered,
  slice-based and modular-monolith styles differ more in vocabulary than in substance.
- Treat the allowed edges as a contract and check the real graph against it.
- Report cycles usefully: state how big each cycle is and which edges, once cut, would dissolve
  it. Cycles inside one component are warnings; between components they are violations.
- Depend toward stability. Instability is efferent coupling divided by afferent plus efferent
  coupling; flag edges to a less stable component, ranked by the margin.

**Keep volatile details away from policy**

- Stable policy must not rely on frameworks, storage, transports, cloud SDKs or UI. Count
  feature-conditional edges as real dependencies.
- Interfaces and traits are one way to invert a dependency. So are passing plain data over the border,
  function parameters, generics, messaging, linking at build time and registries of plugins. Relying
  on the standard library, or on an immutable concrete value, is acceptable.
- Boundaries cover representation too. The worse leak is an infrastructure type in a domain
  signature (ORM entities, request objects, transaction handles, framework annotations), not just
  an import. Work from type-resolved facts and a pattern allowlist rather than import text alone,
  and record explicit exceptions such as annotation-only derives on value types.
- Keep each component's public API small and never re-export third-party types through it.
- Do all concrete wiring in one composition root per executable; domain code never looks in a
  container for what it needs.

**Ownership and indirection**

- Each context owns its data. Shared tables, long synchronous call chains and lockstep releases
  are the marks of a distributed monolith. Where teams meet, agree contracts rather than sharing innards.
- Keep a layer or abstraction only if you can name its purpose: another implementation or a test
  double that is really used, a part you expect to change, a team or release seam, or a rule it
  protects.
- A single deployable with enforced internal boundaries is a sensible default; split into
  services when scaling, deployment or ownership pays for the operational cost.

## Background and lineage

Lakos's levelization; Martin's three package principles (no cycles, depend on the stable, stable means abstract); Cockburn's ports and adapters; Palermo's onion; Martin's clean
architecture; Evans's bounded contexts; Conway's law tying boundaries to ownership; microservices
(Lewis and Fowler) then the swing back toward well-bounded monoliths; vertical slices; and architectural
fitness functions (Ford, Parsons and Kua).

## Measures and numbers

Useful measures include incoming and outgoing coupling, an instability ratio, distance from Martin's main sequence, counts of callers and callees, depth of transitive reach, how many cycles exist and
how big, and counts of boundary crossings. Propagation cost and dependency-structure matrices
grew out of the empirical work above. Abstractness only means something where the language has
first-class abstract types, so prefer instability and the stable-dependencies principle
elsewhere and regard the main-sequence figure as a weak signal.

In Rust, `cargo metadata` gives the exact crate graph of a workspace. Module-level edges need path
resolution; enabling a cargo feature can add edges, dynamic dispatch conceals calls, and macros
or build scripts generate code, so analyse per feature set.

## Cautions

The cargo-cult failure is a request path with a controller, mapper, use case, interactor, port,
gateway, repository and adapter where a short direct path would do. The checker should report
unjustified indirection (ARC-008) rather than reward ceremony.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| ARC-001 | Dependencies follow the declared component graph | A | catalog record; no check yet, needs a declared architecture (`cargo metadata` can run build tooling) |
| ARC-002 | No cycles between components | A | catalog record; no check yet, needs a declared architecture |
| ARC-003 | Stable policy avoids volatile details | A | catalog record; no check yet, needs declared stable/volatile tags |
| ARC-004 | Infrastructure types kept out of policy signatures | P | catalog record; no check yet (planned native check on resolved signatures; needs a forbidden-namespace list) |
| ARC-005 | Depend toward stability | P | catalog record; no check yet (planned: instability from the workspace graph; dependency-cruiser `moreUnstable` for TypeScript) |
| ARC-006 | Component API within budget, no third-party types | A | catalog record; no check yet, needs a declared budget (triage names cargo-public-api, api-extractor, japicmp) |
| ARC-007 | Data owned by one component | A | catalog record; no check yet, needs an owner mapping |
| ARC-008 | Indirection that earns nothing | P | catalog record; no check yet (planned native facts; the justification judgment is a model's) |
| ARC-009 | Architectural drift over time | H | catalog record; no check yet, needs history and waiver data |
| ARC-010 | Changes respect recorded architectural intent | L | catalog record; no check yet, needs a model or human; advisory |
| ARC-011 | Signs of a distributed monolith | H | catalog record; no check yet, needs a service graph, possibly several repositories |
| ARC-012 | Construction only in composition roots | P | catalog record; no check yet, needs declared roots |

Classes: P parser-level facts, A needs a declared architecture, H needs history, L needs a
language model.

## Further reading

- John Lakos, *Large-Scale C++ Software Design*
- Robert C. Martin, *Clean Architecture* and his package principles
- Alistair Cockburn, "Hexagonal Architecture" (Ports and Adapters)
- Jeffrey Palermo, "The Onion Architecture"
- Eric Evans, *Domain-Driven Design*
- James Lewis and Martin Fowler, "Microservices"
- Melvin Conway, "How Do Committees Invent?"
- Neal Ford, Rebecca Parsons and Patrick Kua, *Building Evolutionary Architectures*
- MacCormack, Rusnak and Baldwin, their 2006 study of the dependency structure of open source and proprietary designs
