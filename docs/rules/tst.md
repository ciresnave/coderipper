# TST: trusting your tests

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Tests are code, and they fail in their own ways. They can pass while checking nothing, break on every
refactor, become unreliable, take too long, or just echo the implementation they were written from. The key point is that
coverage tells you what ran, not what would be noticed if it were wrong. The evidence is empirical (E):
Inozemtseva and Holmes (2014) found coverage only weakly related to suite effectiveness once size is
controlled for, Just and colleagues (2014) showed mutants are a fair stand-in for real faults, and Luo and
colleagues (2014) documented how widespread flaky tests are.

## How to apply

**Aim tests at behavior.**
- Test through public interfaces; tests that pin internal details block refactoring.
- Give each test one reason to fail and a name that states the behavior. Structure each as setup, action,
  check. Inside tests, readable and explicit (DAMP) beats deduplicated (DRY).
- Every bug fix brings a regression test.

**Keep runs repeatable.**
- Take charge of the clock, random numbers, ordering, network and disk so results do not depend on run order
  or on concurrency.
- Quarantine flaky tests, naming who owns each and by when it must be fixed, rather than quietly rerunning them.

**Choose doubles and layers deliberately.**
- Favor fakes for the boundaries you own, use mocks only occasionally, and avoid asserting on internal calls.
  At real boundaries, use real dependencies (containers, or in-memory equivalents with known limits).
- Unit tests cover logic, integration tests cover seams, contract tests cover links between components or
  repositories, and a handful of end-to-end tests cover the journeys that matter most.

**Measure strength, not volume.**
- Look at changed-line coverage and mutation results instead of asking whether TDD was followed.
- Add property-based testing and fuzzing for input-facing code such as parsers, codecs and protocol handlers,
  and for modules heavy in unsafe.

## Background and lineage

The practical lineage includes xUnit and test-first work (Beck), the test pyramid (Cohn) and the testing
trophy, the mockist versus classicist debate (Fowler's "Mocks Aren't Stubs"), test-smell catalogs (van
Deursen et al.; Meszaros), mutation testing, property-based testing, coverage-guided fuzzing,
consumer-driven contract tests such as Pact, Google's small/medium/large test sizes, and hermetic tests.

## Measures and numbers

- Gate on coverage of changed lines and branches; report the absolute total without gating on it.
- Gate on mutation score for changed code, and list the surviving mutants. Run the full-repository mutation
  sweep on a schedule, not on every pull request.
- No fixed numeric threshold is built in; the default is configurable per project.

## Cautions and defaults

- Do not impose a fixed absolute coverage target; it rewards trivial tests. Report the total, gate on the diff.
- Do not run mutation testing over the whole repository on every pull request. Limit it to the diff, with a complete sweep on a schedule.
- Treat the pyramid as a cost-versus-confidence trade-off, not a required ratio.
- Three records are marked contested because respected schools disagree. TST-001 and TST-002: some teams demand a fixed line-coverage gate while others note coverage predicts fault detection poorly; the default gates changed-code coverage and mutation score and merely reports the absolute figure. TST-004: mockist teams verify interactions with collaborators while classicists use real collaborators and check state; the default prefers fakes for owned boundaries and judges tests by mutation score rather than style.

## Where it is checked

Only five checks exist today, none of them in this domain, so no rule below is run by the current release. Every row is a catalog record with no check yet; most need tests to be executed.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| TST-001 | Changed-code coverage meets threshold | X | catalog record; no check yet, needs test execution and a ratchet-style gate (cargo-llvm-cov, JaCoCo, coverage.py with diff-cover, Istanbul/c8) |
| TST-002 | Mutation score on the diff meets threshold | X | catalog record; no check yet, needs test execution (cargo-mutants, PIT, Stryker, mutmut, Mull) |
| TST-003 | No empty, trivial or non-calling tests | P | catalog record; no check yet, needs a parser, and per-test coverage attribution for the calling part |
| TST-004 | Over-mocking heuristics in tests | P | catalog record; no check yet, needs a parser for AST heuristics |
| TST-005 | Intermittent tests are assigned and time-boxed | X | catalog record; no check yet, needs CI history from a service (network) |
| TST-006 | Tests hermetic: no network, clock or shared filesystem | X | catalog record; no check yet, needs runs with network off and time frozen |
| TST-007 | Bug fixes ship a regression test | H | catalog record; no check yet, needs a human decision, classifying the commit then inspecting the diff |
| TST-008 | Test names agree with assertions | L | catalog record; no check yet, needs an LLM reading |
| TST-009 | Risky modules have fuzz or property targets | P | catalog record; no check yet, needs module classification by tags or LLM |
| TST-010 | Cross-component interfaces have two-sided contract tests | X | catalog record; no check yet, spans repositories (Pact) |
| TST-011 | Test order and parallelism do not change results | X | catalog record; no check yet, needs randomized and parallel runs (cargo-nextest, pytest-xdist) |
| TST-012 | Test duration budgets with size classes | X | catalog record; no check yet, needs CI timing |
| TST-013 | Expected values not derived from the implementation | L | catalog record; no check yet, needs an LLM reading plus mutation results (see AIH-005) |

## Further reading

- Kent Beck, *Test-Driven Development: By Example*
- Mike Cohn, *Succeeding with Agile*
- Martin Fowler, "Mocks Aren't Stubs"
- Gerard Meszaros, *xUnit Test Patterns*
- Laura Inozemtseva and Reid Holmes, "Coverage Is Not Strongly Correlated with Test Suite Effectiveness"
- Rene Just et al., "Are Mutants a Valid Substitute for Real Faults in Software Testing?"
- Qingzhou Luo et al., "An Empirical Analysis of Flaky Tests"
