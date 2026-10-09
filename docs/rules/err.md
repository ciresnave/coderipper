# ERR: failing well

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Things go wrong in every running system, so the real design questions are about ownership and shape: which
part of the program answers for each kind of failure, how the failure is represented, and whether the
program's invariants still hold afterwards. Three habits cause most of the damage: errors that are swallowed,
errors reduced to free text, and errors that lose their context on the way up. Each leads to silent
corruption, or to an incident nobody can explain later. The evidence is mostly consensus (strength C),
supported by studies of how exceptions get misused.

## How to apply

**Decide what kind of failure you have.**
- A violated invariant or programmer error is a bug. Halt promptly where responsibility for the task begins (assert, panic, abort) and put supervision or isolation around it, so a single task cannot bring down the whole process.
- An expected runtime failure (I/O, user input, a remote error) gets a typed value that is either handled or
  passed upward with context.
- Running out of a resource calls for a stated policy, whether that is dropping work, degrading service or refusing requests.
- State fail-fast carefully. The aim is to spot an invalid state as early as it can safely be judged invalid,
  keep it from spreading, and tell the owner. A crash is only one way to do that; an `Err`, an HTTP 400, a
  transaction rollback and a dead-letter queue all qualify. What matters is that invariants survive and the
  owner hears about it.

**Make errors visible and useful.**
- Skip empty handlers, ignored results whose outcome matters, and blanket catches that hide the cause. Also
  avoid logging and rethrowing at every layer, which only multiplies noise.
- Include the operation, the resource and the cause chain. Libraries expose typed, matchable, stable errors;
  applications combine them and add context. Callers should never need to parse a message.
- Treat `unwrap` and `expect` as assertions about invariants and justify them; external input must never
  trigger a panic.

**Respect boundaries and leave things consistent.**
- A domain error is not an HTTP status. Convert in the adapter, and keep internals and secrets out of what
  crosses the boundary.
- Clean up on every path with RAII, `defer`, `finally` or `using`, and give multi-step state changes a
  rollback or compensation.

## Background and lineage

The history is a progression: return codes in C, then exceptions (CLU, C++, Java, and the long argument over
checked exceptions), then result-style and either-style types (ML, Haskell, Rust, Swift) alongside Go's
explicit error values. Along the way came Meyer's separation of a contract violation (a bug) from an ordinary
failure, the "let it crash" supervision model from Erlang/OTP, Shore's fail-fast article, Ousterhout's advice
to design errors out of existence, and Nygard's stability patterns.

## Measures and numbers

This domain has no numeric thresholds. Findings are presence checks (a dropped result, a catch-all outside a
boundary function) and are reported per site.

## Cautions

Do not treat every error as fatal, and do not write defensive handlers for situations the design already rules out: dead handlers hide real bugs. A tool can usually show that an error is dropped; it cannot tell
whether the handler that replaced it is any good, which is why several rules below need review. Exceptions versus result types is a standing disagreement: one camp values clean happy paths, the other wants failure visible and composable. The default is to follow the language's idiom and stay consistent within a codebase, and the rules are phrased for both; ERR-004 is marked contested for this reason.

## Where it is checked

Only five checks exist today, and none of the rules below is run by the current release except through the legacy check `unused-return-values`, which partly overlaps ERR-001 and ERR-011: it reports a function whose result every caller discards, a narrower question than whether any particular failure is dropped. Every other row is a catalog record with no check yet; "planned" names the tool or native check the triage proposes.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| ERR-001 | Errors are not swallowed | P | catalog record; partial overlap with the legacy check `unused-return-values`; otherwise no check yet. Planned: Clippy `let_underscore_must_use` with rustc `unused_must_use` (Rust), ESLint `no-empty` and `no-floating-promises` (TypeScript), Ruff `S110`, `BLE001`, `E722` (Python) |
| ERR-002 | Catch-all handlers only at boundaries | P | catalog record; no check yet. Planned: native AST check for Rust and TypeScript, Ruff `BLE001` for Python, plus a boundary allowlist |
| ERR-003 | No panics or unchecked indexing on input paths | P | catalog record; no check yet. Planned: Clippy `indexing_slicing` (Rust), `noUncheckedIndexedAccess` (TypeScript, indexing only); Python would not be covered, since it raises rather than panics |
| ERR-004 | Library errors are typed so callers can match them | P | catalog record; no check yet. Planned: native signature check for Rust and TypeScript, Ruff `TRY002` for Python |
| ERR-005 | Cross-boundary errors keep context and cause | P | catalog record; no check yet. Planned: ESLint `preserve-caught-error` (TypeScript), Ruff `B904` (Python); Rust has no deterministic tool, judging context needs review |
| ERR-006 | Domain errors free of transport details | A | catalog record; no check yet, needs a declared architecture (ties to ARC-004) |
| ERR-007 | Resources released on error paths | P | catalog record; no check yet. Planned: Ruff `SIM115` for Python only; no mature analyser exists for Rust or TypeScript |
| ERR-008 | Error branches covered by tests | X | catalog record; no check yet, needs executed tests with fault injection or error-arm coverage |
| ERR-009 | Errors and logs leak no sensitive data | P | catalog record; no check yet. Planned: native taint heuristic for TypeScript and Python; none identified for Rust |
| ERR-010 | Compound state changes say whether they are all-or-nothing | L | catalog record; no check yet, needs an LLM reading to find multi-write sequences |
| ERR-011 | Non-throwing parse results are checked | P | catalog record; partial overlap with the legacy check `unused-return-values`; otherwise no check yet. Planned: rustc `unused_must_use` (Rust), native check on `parseInt`, `Number`, `JSON.parse` (TypeScript), pyright `reportOptionalMemberAccess` (Python) |

## Further reading

- Bertrand Meyer, *Object-Oriented Software Construction*
- Joe Armstrong, *Making Reliable Distributed Systems in the Presence of Software Errors*
- Jim Shore, "Fail Fast"
- John Ousterhout, *A Philosophy of Software Design*
- Michael Nygard, *Release It!*
