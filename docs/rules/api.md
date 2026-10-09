# Promises you make by publishing: evolving interfaces safely

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Whatever you release becomes a commitment: a function in a library, an endpoint on a service, a data schema, a file or wire
format, a command-line flag, a configuration key. Users will come to lean on whatever they can observe, including details you
never meant to promise; Hyrum's Law is the name usually given to that tendency. So a design is judged by more than its present
shape: it must also be able to change without breaking consumers. Incompatible changes rank among the main sources of ecosystem pain, and the
kind a tool can detect is cheap to stop before release.

## How to apply

### Shape the surface

- Publish only what you meant to publish, with documentation. Do not name another library's types in your signatures, since their version policy
  then becomes yours.
- Mark where growth is expected: enums and structs marked non-exhaustive, sealed traits and feature-gated extension points let you
  add later without a breaking release.
- Where order or timing is unspecified, consider varying it deliberately so nobody builds on an accident.

### Change it in a disciplined way

- Add in minor versions and break only in majors. Removals follow a deprecation lifecycle: the notice names the version it began in,
  the replacement and the planned removal. Some services also expose dated API versions, so each consumer can stay on the day it
  integrated.
- Follow schema evolution rules, as set out by Protocol Buffers, Avro and JSON Schema: do not recycle a field number or change its
  type, add optional fields rather than required ones, define what happens to unknown fields, and state which older and newer
  versions must interoperate.
- Migrate databases in expand and contract steps: first add, then backfill, then switch over, and only last remove, with every step
  working for the previous application version.
- Emit data strictly, and accept it only within explicit, bounded limits. Silently tolerating garbage means preserving that tolerance
  forever.

### Pin it with tests and tooling

- Stored sample files (golden files) and write-then-read tests protect wire and file formats.
- Documentation examples are compiled as doctests; command-line flags, configuration keys and exit codes get snapshot tests.
- Compare each release's public interface with the last, and review the snapshot whenever it changes.

## Background and lineage

The thread runs from Parnas's work on interface specification through Bloch's talk "How to Design a Good API and Why It Matters"
(2006) and Semantic Versioning (2013). Postel's robustness principle (1981) is now criticised because liberal acceptance ossifies
protocols. Practical descendants include the Rust API Guidelines, the Go 1 compatibility promise, Rust editions, the Pact project built on
consumer-driven contracts (Robinson, 2006), and the parallel-change pattern (Sato, 2014).

## Measures and numbers

No numeric thresholds are set. The compatibility window is the one quantity to decide: a schema policy states explicitly whether
version N-1 and N+1 readers must work.

## Cautions

Over-application is the main risk: treating internal interfaces as public and freezing them, or running versioning ceremony for a
component whose single consumer can be changed in the same commit. Apply these rules to surfaces with outside consumers. Postel's
law is contested: one side favours generous input handling, the other says strict parsing prevents ossified protocols and attacks.
The default is strict parsing with any tolerance explicit, documented and bounded; no single API rule carries the dispute, so none
is flagged contested. Several checks need the project to be compiled or run, so they execute project code.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| API-001 | Breaking public changes since last release | P | no check yet (catalog record only; planned: cargo-semver-checks (Rust), griffe (Python), api-extractor (TypeScript, reports changes only)) |
| API-002 | Public surface snapshot reviewed when changed | P | no check yet (catalog record only; planned: cargo-public-api, api-extractor, griffe dump; the review itself is human) |
| API-003 | Version number matches kind of change | P | no check yet (catalog record only; planned: compare the bump with the API-001 result) |
| API-004 | Deprecation notices carry full lifecycle data | P | no check yet (catalog record only; planned: parse deprecation attributes and docs) |
| API-005 | Serialized formats tested for old and new readers | X | no check yet (catalog record only; needs the project's tests to run) |
| API-006 | Schema changes obey evolution rules | N | no check yet (catalog record only; planned: buf for protobuf; other formats need their own tools) |
| API-007 | Database migrations safe step by step | P | no check yet (catalog record only; planned: squawk (Postgres); sequencing review needs an LLM) |
| API-008 | Public API exposes no third-party types | P | no check yet (catalog record only; planned: scan public signatures; related to ARC-006) |
| API-009 | Extension points flagged on exported enums and structs | P | no check yet (catalog record only; planned: clippy restriction lints (Rust); no equivalent in TypeScript) |
| API-010 | Doc samples build and execute | X | no check yet (catalog record only; needs the project's tests to run) |
| API-011 | Command-line, config and exit-code surfaces under test | X | no check yet (catalog record only; needs the project's tests to run) |
| API-012 | Unpromised behaviour not accidentally relied upon | L | no check yet (catalog record only; compares documented guarantees with behaviour; needs an LLM) |

The legacy checks do not overlap with this domain. Tool names come from CodeRipper's own triage; none is run by the current
release.

## Further reading

- "On the Criteria To Be Used in Decomposing Systems into Modules" and later interface-specification work, by David Parnas.
- "How to Design a Good API and Why It Matters", Joshua Bloch.
- Semantic Versioning, Tom Preston-Werner.
- Rust API Guidelines; the Go 1 compatibility promise; Rust editions.
- Consumer-driven contracts, Ian Robinson; the Pact project.
- Parallel change (expand and contract), Danilo Sato.
- Postel's robustness principle and its critiques.
