# What changed for consumers after 0.15.0

For a publish decision. Source: `CHANGELOG.md` at `main` = 0.17.4. Per the PM (2026-10-31), crates.io holds 0.15.0; I did not read the registry, so the baseline is that statement. Nothing here is published, tagged or proposed for a release; every number below is the version of `main` at a merge.

## Breaking (pre-1.0, so these are the second number: 0.16.0 and 0.17.0)

| Version | What a consumer sees | Migration |
|---|---|---|
| 0.16.0 | `reachability` and `unused-return-values` findings carry a **qualified `subject`** (`S::new`, `m::S::method`), as `unused-parameters` already did | An `[[allow]]` `symbol` or `[reachability] public_api` `symbol` naming a method, field or variant must become the qualified name. A stale entry is reported as stale, not silently dropped. A top-level item keeps its bare name |
| 0.17.0 | New default-on rule **RDB-008** (`TODO`/`FIXME` in a comment with no owner or ticket), in `portfolio` and `extended`. Low, one finding per file, experimental so reported and never gating | None needed. A repository with bare to-dos gains findings; owning them (`TODO(name)`, `@name`, `#123`, `PROJ-123`, a link) or an `[[allow]]` on the file clears them |

Already in the 0.15.0 you have: `security-support-statement`, default on, same kind of change.

## New behaviour, not breaking

- **Exit codes.** `--min-coverage <pct>` (0.15.2) exits **1** when any reported language's coverage is below the floor; `pct` outside 0 to 100 is exit 2. `max_runtime_minutes` is now **enforced** (0.15.7): when the ceiling passes, the process stops with a stderr line and exit **3** (a check could not run). Child processes already spawned are not killed. The end-to-end stop at real minutes was not run, only a unit test.
- **Command line.** `--manifest-path <Cargo.toml>` (0.15.3, conflicts with `--project` and `--scope`), `-p/--package <name>` (0.15.4, repeatable, implies `--workspace`), `cargo coderipper --sweep` and `--check <id>` (0.15.5; both together or `--check` without an id is exit 2).
- **Output.** `--message-format short` (0.15.6): one `file:line: severity: summary [check id]` line per finding, stdout only. `human` and `json` unchanged.
- **Configuration.** The dotless `/etc/coderipper/coderipper.toml` is read, Unix only (0.15.9).
- **Fewer false findings.** RDB-008 ignores a marker inside a single-line string literal and accepts `TODO [name]` (0.17.1). A string spanning lines is still not tracked.
- **Error text.** A failed link names the linker's own message (`cannot find -l...`) instead of the `cc` command line (0.17.3).

## Library

`CheckContext::packages(..)` (0.15.4) is a new field on a `#[non_exhaustive]` struct (per the changelog). I did not run a semver check against 0.15.0, so whether it needs more than a minor bump is not established here.

## No consumer effect

0.15.1 (a flaky test), 0.15.8, 0.16.1, 0.17.2 and 0.17.4 (docs only).

## Not measured

No `cargo semver-checks` or `cargo package` run against 0.15.0 was made for this note; the patent design rules the PM named (E1/E2) are not part of it.
