# Adding behaviour safely: the EXT rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Needs change, and editing code that is stable and tested carries risk. Yet flexibility added on a
guess costs about as much as rigidity does. Two things follow. Extension points belong where
variation has actually been observed, and every implementation sitting behind an abstraction must
keep that abstraction's promises. Agreement is strong that substitutability in Liskov's sense is
a correctness condition; evidence for the open-closed principle as a design rule is weaker.

## How to apply it

**Choose how the code may vary**

- For an open set of variants use interfaces, traits or plugins. For a closed set use an enum or
  sealed hierarchy with exhaustive matching, so adding a variant turns every spot needing an
  edit into a compile error.
- Object-oriented designs let you add types cheaply but operations dearly, and sum types do the
  reverse; both cannot be had at once (the expression problem).
- Use whatever mechanism the language offers: interfaces, traits, protocols, abstract base
  classes, sum types, higher-order functions, generics, configuration or data-driven behaviour,
  plugin registries, message passing. Openness to extension is something to aim for; it
  never obliges you to use inheritance.

**Wait for evidence**

- Both the rule of three and YAGNI advise: hold off until a second or third real variant exists,
  unless an outside force such as an unstable vendor or a published plugin interface justifies going earlier.
- Metz's point is that duplication costs less than the wrong abstraction.
- Do not make an interface for every class.

**Keep implementations honest**

- An implementation must take every input the abstraction allows, give back everything the
  abstraction guarantees, preserve its invariants, and fail only in declared ways. Warning signs
  are "not supported" errors, empty overrides, and clients that switch on the concrete type.
- Run one shared contract suite against every implementation.
- Keep interfaces narrow: clients should depend on the small role they use, judged by actual
  client usage as well as member count.

**Inheritance**

- Prefer holding a collaborator to inheriting its code. Inherit only where a true is-a relation
  holds, with a written-down extension contract, and close types to subclassing unless told otherwise. Bloch's
  advice is to design for inheritance or forbid it.

## Background and lineage

Meyer's 1988 open-closed principle relied on inheritance; Martin's 1996 reading rests on abstract
interfaces instead. Other threads are behavioural subtyping (Liskov, 1987; Liskov and Wing,
1994), interface segregation, the Gang of Four patterns (Strategy, Template Method, Decorator,
Visitor), composition over inheritance, Wadler's expression problem, and Metz (2014).

## Measures and numbers

Depth of inheritance, which base classes are visible across modules, per-client member usage of
interfaces, count of empty implementer bodies, and the number of files touched when a variant is
added (the limit is configurable).

## Cautions

An interface for every class, layers of factories and type parameters that never vary are the
failure on the other side. The checker should be as willing to call an abstraction unjustified as to call
one missing. Several rules are advisory or low confidence because they
depend on judgment about intent (EXT-010) or on history (EXT-007 to EXT-009).

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| EXT-001 | Exhaustive matching on closed variant sets | P | catalog record; no check yet (triage names the Clippy restriction lint `wildcard_enum_match_arm`, typescript-eslint `switch-exhaustiveness-check`, pyright `reportMatchNotExhaustive`) |
| EXT-002 | No downcasts on abstractions | P | catalog record; no check yet (planned native search for downcasts and type tests in client code) |
| EXT-003 | Implementations honour their contract | P | catalog record; no check yet (triage names Clippy `unimplemented` and `todo` for Rust; native checks for the others; no-op overrides in Rust are not detected) |
| EXT-004 | Fat interfaces | P | catalog record; no check yet, needs per-client usage analysis (not worth building yet for Rust) |
| EXT-005 | Bounded inheritance | P | catalog record; no check yet (pylint too-many-ancestors for Python; not applicable to Rust, which has no implementation inheritance) |
| EXT-006 | Shared contract tests for every implementation | X | catalog record; no check yet, presence is static but passing needs running tests (Pact for cross-component) |
| EXT-007 | Speculative generality | P | catalog record; no check yet (planned native facts; history shows what was never extended) |
| EXT-008 | Shotgun surgery when adding a variant | H | catalog record; no check yet, needs history mining |
| EXT-009 | Stable components edited for new variants | H | catalog record; no check yet, needs history and stable tags |
| EXT-010 | Abstraction cuts along the true dimension of variation | L | catalog record; no check yet, needs a model or human; advisory |

Classes: P parser-level facts, X needs executing tests, H needs history, L needs a language model.

## Further reading

- Bertrand Meyer, *Object-Oriented Software Construction*
- Robert C. Martin, writings on the open-closed and interface segregation principles
- Barbara Liskov, "Data Abstraction and Hierarchy" (OOPSLA 1987 keynote); Liskov and Wing, "A Behavioral Notion of Subtyping"
- Gamma, Helm, Johnson and Vlissides, *Design Patterns*
- Joshua Bloch, *Effective Java*
- Philip Wadler, the "expression problem" note
- Sandi Metz, "The Wrong Abstraction"
