# Changelog

All notable changes to CodeRipper. Versions follow the portfolio rule: **every pushed change changes the version;
a breaking change changes the major version, and before 1.0 the major is the second number** (0.n.x). None of these
versions has been published to crates.io or tagged on GitHub yet: they are the versions of `main` at each merge.

## 0.3.2 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::catalog`**: the rule catalog of the multi-language design (P2, first PR). A `Rule` is a record in CodeRipper's own
  words (statement, rationale, scope, network, unit, default severity, `kb_refs`, aliases, lifecycle); `Catalog::builtin()` loads the
  records under `rules/`, and `Catalog::parse` loads any set of record files strictly (an unknown or missing field, a blank statement, an
  ID or alias claimed twice is an error naming the file). The five existing checks have records, and a test fails if a record and its
  check disagree about scope, network or unit.
- `Scope`, `Unit` and `Network` can be deserialized (`local` / `network` for `Network`).
- No behaviour change for the command or `run_checks`: nothing reads the catalog yet.

## 0.3.1 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::module`**: the module protocol of the multi-language design (P1). A `Module` answers `describe` and `check`; the five
  built-in checks now run behind `RustModule`, and `ExternalModule` speaks the protocol to a child process (JSON lines, a scrubbed
  environment, wall-clock and output limits, the whole process tree killed on a timeout). `reconcile` is what the host believes: a rule
  that got no verdict (crash, hang, silence, a wrong count, a missing summary) is an error, never a clean run. `run_module` runs any
  module through the same validation and allowlist as `run_checks`.
- No behaviour change for the command or `run_checks`: every existing test passes unedited and the JSON output of a fixed fixture is
  byte-identical before and after. Nothing selects an external module yet (discovery and the catalog are later phases).

## 0.3.0 - 2026-10-06

CodeRipper becomes three things at once: a library, a command that is also a cargo subcommand, and (planned, not in this
crate) a hosted service. This is the library's first release.

### Breaking
- **The data types are `#[non_exhaustive]`**: `Finding`, `Location`, `Severity`, `Confidence`, `FindingError` (and its variant),
  `CheckContext`, `Scope`, `Unit`, `Network`, `Tier`, `ApiError`, `CacheConfig`, `RunResult`, `WorkspaceRun`, `RepoRef`, `GhCli` and the
  four unit-struct checks, so a field or variant can be added later without breaking anyone. Build them with `Finding::new` +
  `.location()` / `.subject()` / `.positive_control()` / `.member()`, `Location::new`, `CheckContext::new`, `ApiError::new`,
  `RunResult::new`, `RepoRef::new`, `CacheConfig::new` + `.with_wait()` / `.with_max_bytes()`, and `ReachabilityCheck::new()` (and the
  other built-in checks), and match enums with a wildcard arm.
- **`Check` and `Github` are `Send + Sync`**, so checks run on a thread pool or in a service. A method added to either later will
  have a default body.
- **`CheckContext::new(project_root)`** takes the project alone; the portfolio root (which no built-in check reads) defaults to the
  project's parent and is set with `.portfolio_root(path)`. `Check::scope()` has a default (`Scope::Project`).
- **An unknown check id is an error**, no longer a clean empty run: the library returns it in `errors`, the command exits 2 with the
  list of checks. Two checks sharing an id are refused.
- **Exit codes.** `coderipper` now exits 0 (clean), 1 (a finding at or above `--deny`), 2 (usage error: bad flag, unknown check id,
  a project path that cannot be read) or 3 (the audit could not be completed: a check could not run, or the command failed). Before, a
  check that could not run and a bad project path both exited 1, and findings could not fail a run at all.
- The command-line plumbing in the library (`UnitFilter`; in `build_cache`: `config_from_env`, `status`, `prune_to_cap`, `take_stats`,
  `render_stats`, `MARKER`, `RepoDirInfo`, `BuildStats`) is no longer public (it was only ever for the binaries).

### Added
- **`run_checks_with`**: run your own `Check` (or any list of checks) through the host's validation and the project's allowlist.
  `run_checks` is now a wrapper over it.
- **`cargo coderipper`**: a second binary; a bare `cargo coderipper` is `fast`.
- **`--deny <info|low|medium|high|critical>`** on `fast`, `sweep` and `check`. Off by default (findings print but never fail the run, like
  clippy warnings); CI writes `--deny medium`.
- **`--message-format json`**: one JSON object per line with a `"reason"` (`coderipper-finding`, `coderipper-summary`).
- `--version`; `coderipper::anyhow` and `coderipper::serde_json` (re-exports of the types in the trait signatures); `Debug` / `Clone` /
  `PartialEq` / `Hash` where a library user expects them.
- A `cli` cargo feature (default on) gates `clap` and the two binaries: `default-features = false` is the library alone.
- Crate documentation, `#![warn(missing_docs)]`, three runnable examples (each run by a test), golden files for the `Finding` JSON and
  the `.coderipper.toml` format, and `rust-version = "1.89"` (where `File::try_lock`, used by the build cache's lock, was stabilised;
  1.88 fails to compile it, measured).
- CI jobs: docs (`-D warnings`), the library alone, MSRV, `cargo package`, and cargo-semver-checks on pull requests.

### Changed
- The absence-claim rule in `Finding::validate` matches whole words: "10 threads", "version 1.0" and "casino" are no longer rejected as
  claims of an absence.
- The `serve` subcommand, which only ever said "not implemented", is hidden from `--help`.
- The package no longer ships `docs/`, `.github/`, `codecov.yml` or `tests/`; the README, the package description and the command's
  `--help` no longer promise dependency checking or a server, which do not exist.

### Fixed
- Windows: the `\?\` prefix of a canonicalized path no longer appears in messages and JSON.

## 0.2.13 - 2026-10-04

### Changed
- Dependency `toml` 0.9 to 1.

## 0.2.12 - 2026-10-03

### Added
- **`--workspace`** on `fast`, `sweep` and `check`: runs the checks over every member of the cargo workspace containing the project (the root or any
  member's directory). Checks that judge the whole repository (`version-consistency`, `ci-protection-presence`) run once, at the workspace root;
  checks that judge a package run once per member, in sorted order, each exactly as `--project <member>` runs them. Findings carry a new optional
  `Finding.member` (the cargo package name) and lead with it. A member that fails does not stop the others (the run still exits non-zero). stderr shows
  `coderipper: member <name> (i/n)` per member and a one-line summary.
- **Cost to know about:** while a `--workspace` run is in progress it holds the build cache's lock for that repository, so another coderipper run on the
  same repository builds without the cache (correct, slower) and says so. Not measured yet on a large workspace.

## 0.2.11 - 2026-10-03

### Added
- The session checkout used by `--workspace` (no command used it yet in this version): one git checkout for a whole run, each member rewritten in place and
  restored with `git checkout`, never with an old mtime (which would let a dependent link against the rewritten build); a failed restore is retried, then
  stops the run; the session holds the cache lock for the run and builds in `<cache dir>/target`.

### Fixed
- Two tests that CI found over-strict: one matched a random temporary directory name (a flake that could fail any run, not caused by this version), and one
  assumed a git behaviour (a file rewritten with identical bytes gets a new mtime on restore) that holds on git 2.55 but not on CI's git versions.

## 0.2.10 - 2026-10-03

### Added
- `Check::unit()` (`Unit::Package` or `Unit::Repository`) says whether a check judges one cargo package or the whole repository
  (`version-consistency` and `ci-protection-presence` judge the repository), `UnitFilter` lets the host run one kind or the other, and
  `package::workspace_members` lists a workspace's members in a stable order. Groundwork for `--workspace`; no CLI change yet.

## 0.2.9 - 2026-10-03

### Fixed
- **A build could be served another CodeRipper run's compiled copy of the package it was analysing (affects 0.2.7 and 0.2.8
  only).** Each check makes its checkout and rewrites it *before* taking the build-cache lock, and cargo calls a unit fresh when
  no source file is newer than the cached artifact. A run that waited for the lock (or was slow to reach it) while another run
  built the same unit into the shared cache therefore reused that run's compile, rewrite and diagnostics, and reported that
  run's findings with no error. The merged code was **reproduced** to do this (a test with two checkouts of one repository and
  a shared cache: the second build reported the first build's unit as fresh, for two builds of the same kind and for a
  `--lib` build followed by an `--all-targets` build). It needs two separate `coderipper` processes overlapping on the same
  repository; one process running several checks in a row, and any run with `CODERIPPER_CACHE=off`, were not affected.
  Right after taking the lock a run now sets every file of its checkout to the current time, so cargo recompiles the package.
  If that is impossible the run builds without the cache, releases the lock, and says why.
  **If you ran two overlapping runs on one repository with 0.2.7 or 0.2.8, discard the later run's findings and re-run it alone.**

## 0.2.8 - 2026-10-03

### Changed
- Codecov's pull-request statuses are informational: coverage is a signal, not a gate (`codecov.yml`). `coverage.yml` says what the
  number covers.

## 0.2.7 - 2026-10-03

### Added
- **Shared dependency-build cache.** The checks that compile your package build in a persistent target directory per repository and
  toolchain, outside your project (`CODERIPPER_CACHE_DIR`, else `%LOCALAPPDATA%\coderipper\build`, `$XDG_CACHE_HOME/coderipper/build`
  or `~/.cache/coderipper/build`), guarded by CodeRipper's own lock with a bounded wait, with a size cap (`CODERIPPER_CACHE_MAX_GB`,
  20) and `coderipper cache status|prune`. Measured on `fuel-core`: 234 s cold, 90 s warm. `CODERIPPER_CACHE=off` disables it. Each run
  that built something ends with one stderr line (`coderipper: cache <dir> - N units fresh, M compiled`).
  *Known defect in this version: see 0.2.9.*

## 0.2.6 - 2026-10-02

### Added
- A separate, non-required `Coverage` workflow (cargo-llvm-cov, uploaded to Codecov).

## 0.2.5 - 2026-10-02

### Added
- **`ci-protection-presence`** (sweep tier, network): reports a repository whose default branch requires no status checks. Reads
  `enforcement_level` and the required contexts, and repository rulesets, never GitHub's `protected` flag (which is true for branches that
  enforce nothing).

## 0.2.4 - 2026-10-02

### Added
- **`version-consistency`**: the packages of a project share one version, except a package that tracks another project's (`[[tracks]]`).

## 0.2.3 - 2026-10-02

### Added
- A workspace member can be a project (`--project <member directory>`): the checkout holds the whole repository, only the member is
  rewritten and built, and findings are package-relative.

## 0.2.2 - 2026-10-02

### Changed
- `reachability` analyses packages with a library plus binaries: it builds only the library and does not report an item that a bin, test,
  example or bench reaches by name.

## 0.2.1 - 2026-10-02

### Added
- **`unused-parameters`** (Rust): function parameters rustc's `unused_variables` reports as unused.

## 0.2.0 - 2026-10-02

### Added
- Allowlist entries that no longer suppress anything are reported as `Info` findings (stale-this-run detection).

## 0.1.0 - 2026-10-01

### Added
- The host (the `Check` trait, the `Finding` schema, the allowlist, the CLI: `fast`, `sweep`, `check`) and the first checks:
  **`reachability`** (dead code, including `pub` items rustc skips) and **`unused-return-values`**.
