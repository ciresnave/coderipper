# Changelog

All notable changes to CodeRipper. Versions follow the portfolio rule: **every pushed change changes the version;
a breaking change changes the major version, and before 1.0 the major is the second number** (0.n.x). None of these
versions has been published to crates.io or tagged on GitHub yet: they are the versions of `main` at each merge.

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
