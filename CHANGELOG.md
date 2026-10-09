# Changelog

All notable changes to CodeRipper. Versions follow the portfolio rule: **every pushed change changes the version;
a breaking change changes the major version, and before 1.0 the major is the second number** (0.n.x). None of these
versions has been published to crates.io or tagged on GitHub yet: they are the versions of `main` at each merge.

## 0.4.1 - 2026-10-09 (proposed)

### Added
- **The delegation framework, and the first delegated rule: SEC-002 (no committed secrets), run by [gitleaks](https://github.com/gitleaks/gitleaks)**
  (multi-language P3, PR 3b). `coderipper::module::DelegatedModule` runs rules through tools the cache installs. It resolves the tool
  through a `tools::ToolEnv` (lock, cache, consent, fetcher), runs it with a scrubbed environment, an empty scratch directory as its working
  directory and the run's limits (the process tree is killed on a timeout), and reads only its machine-readable output. A tool whose
  output cannot be read is `tool_output_unreadable`, never "no findings"; a non-zero exit is `tool_failed` with the tail of stderr.
- **SEC-002 under `--profile extended`**: `gitleaks git` scans the whole history (a secret committed and later deleted is still
  reported), with `--redact`, so the value of a secret is never read into CodeRipper and cannot reach a finding, a log or the JSON output.
  Each leak is a `High`-severity finding at `file:line`, subject the file (so the allowlist can name `check` + `file`), confidence
  `Medium` (`Low` for gitleaks' generic entropy rule). It sees gitleaks' own rule set at the pinned version, and nothing outside git.
- **A missing tool is a coverage gap, never a failure.** Not pinned, no build for this platform, or not installed and not allowed to be
  (`consent_not_given`): the rule is reported as not run (`coderipper: note: SEC-002 not run: ... run: coderipper tools install gitleaks
  --install-tools`), counted as skipped in the coverage line, and the exit code is untouched. An error is reserved for a tool that was
  asked for and could not be had or trusted: a `checksum_mismatch`, a refused archive, an install that failed after `--install-tools`.
  The coverage figure does not count the claim as covered while the tool is not installed on that machine.
- **The analysed repository cannot silence SEC-002** (design 5.2): gitleaks is run with its default rules only, `--ignore-gitleaks-allow`,
  and the repository's **git directory** as its source, so a committed `.gitleaksignore`, `.gitleaks.toml` or `gitleaks:allow` comment is not
  honoured. The project's way to suppress a finding is the `.coderipper.toml` allowlist (check + file), which is visible in review.
- **Scope**: a project in a subdirectory of a repository gets the leaks under it, named relative to it (history is the repository's);
  a path that is not a git repository with a commit is `tool_failed` (exit 3), never a clean rule; a relative `project_root` is made absolute.
- **A shallow clone is a gap too**: gitleaks sees only the commits that were fetched, so an empty report from a shallow repository (CI's
  default checkout) is reported as not run (`git fetch --unshallow`), never as clean. Findings in a shallow clone are still reported.
- A run in which a rule did not run for want of its tool no longer prints a bare `coderipper: no issues found`: it says
  `no issues found by the rules that ran (N not run: see the notes on stderr)`, and the JSON summary line gains a `notes` array
  **only then** (every other run keeps exactly its keys). The coverage line's `not applicable here` count now reads `not run here`.
- **`--install-tools` and `--tools-lock FILE` on `fast`, `sweep`, `check` and `conformance`** (and so `cargo coderipper`). Without
  `--install-tools` a run downloads and writes nothing.
- **gitleaks 8.30.1 is pinned** for `x86_64-windows`, `x86_64-linux`, `aarch64-linux`, `aarch64-macos` and `x86_64-macos`. The archive
  hashes are those in the publisher's `gitleaks_8.30.1_checksums.txt`; each member's hash was computed from the verified archive.
- `coderipper conformance --module delegated` runs the SEC-002 fixtures (`conformance/SEC-002/`, a seeded committed key and a clean twin
  holding a placeholder) against the real tool. Without the tool the rule is `unproven`, not failed. `tests/delegated_gitleaks.rs` installs
  the pinned gitleaks and runs the fixtures and the CLI against it; it needs the network once (`CODERIPPER_SKIP_NETWORK_TESTS=1` skips it).
- Library: `RuleStatus::Unavailable`, `RuleResult::tool_unavailable`, `TOOL_UNAVAILABLE` (only a built-in, in-process module may report a
  tool gap; from an external module it is a `protocol_mismatch`), `RunResult::notes`, `tools::ToolEnv`, `run_checks_in_with_tools` and
  `run_workspace_in_with_tools`. `run_checks_in` and `run_workspace_in` keep their signatures and use `ToolEnv::from_environment()`,
  which has no consent to install.

### Changed
- `coderipper tools list` now lists gitleaks (the shipped lock is no longer empty).

### Known limits
- Only gitleaks is pinned. osv-scanner, lychee, zizmor and buf follow, one per release; checkov (PyPI only) needs a Python-prerequisite decision.
- SEC-002 reads history through git, so it needs a git repository (a project outside git is `tool_failed`); a secret gitleaks has no pattern for is not seen.
- A `Composite` of a `DelegatedModule` with an external module counts as not in process, and then refuses a tool gap as a protocol mismatch: compose built-in modules only.

## 0.4.0 - 2026-10-09 (proposed)

### Breaking
- `coderipper::tools::ToolEntry` and `ToolError` are now `#[non_exhaustive]`, and gained fields (`archive`, `member`, `file_sha256`) and a
  variant (`ArchiveRefused`): struct-literal construction and exhaustive matches outside the crate stop compiling. Pre-1.0, so the
  second number moves (0.3.x was never published). Later additions are non-breaking.

### Added
- **Tools can be installed from an archive** (multi-language P3, PR 3a): a lock entry may name `archive = "tar.gz"` or `"zip"`, the
  `member` to take out (a `/`-separated path of safe components) and `file_sha256`, the member's SHA-256; the three go together or the
  lock is `tools_lock_invalid`. `sha256` is then the archive's (what a publisher's checksum file lists). The archive is hashed before it
  is opened, unpacked in memory, and the member is hashed against `file_sha256` before anything is written; the installed file is
  re-checked against `file_sha256` each time it is resolved. Only the member is written.
- **Archives are refused whole** (`archive_refused`, exit 3 from the CLI) if any entry, wanted or not, has a name that could leave the
  directory (`..`, absolute, a drive, a backslash, a NUL, not UTF-8) or is a symlink, hardlink, device or fifo; so is an archive with two
  entries of the member's name (tar only: the zip reader folds a repeated name into the last entry, so a zip duplicate is not
  reported, but the member's hash still binds what is written) or whose member is not a regular file. A member over the size limit
  is `tool_install_failed`, not truncated; the decompressed tar stream is bounded too, and reaching its bound is an error (a cut on an
  entry boundary would otherwise leave later entries unchecked). An archive that is not the declared kind, or lacks the member, is `tool_install_failed`.
- New dependencies, all pure Rust: `flate2`, `tar` (without `xattr`), `zip` (the library only reads zips; the tests write them).
- `ToolEntry` and `ToolError` are now `#[non_exhaustive]` (they gained fields/variants; this makes the next addition non-breaking).
  Building `ring` needs a C compiler; nothing else here does.
- README: a note that `ring` (C and assembly, through `ureq`'s `rustls`, `cli` feature only) is the one non-Rust dependency, and that
  replacing it is wanted.

### Changed
- `coderipper tools install NAME` with a name the lock does not have is now exit **2** (usage), not 3, and nothing is installed even for
  the valid names given beside it. The message still begins `tool_missing`. A tool known for other platforms only stays exit 3.

### Known limits
- The shipped lock is still empty: the delegations (gitleaks, osv-scanner, lychee, zizmor, buf; checkov needs a pip installer) follow.
- `--install-tools` is still only on `tools install`; it joins the run commands with the first delegation.

## 0.3.7 - 2026-10-09 (proposed)

### Added
- **The tool cache** (multi-language P3, PR 2): the isolated place, and the policy, for the external tools delegated rules will use.
  `coderipper::tools` (`ToolsLock`, `ToolCache`, `Consent`, `Fetcher`, `DefaultFetcher`, `ToolError`) and `coderipper tools list` /
  `coderipper tools install [NAME...] --install-tools [--tools-lock FILE]`.
  - Tools live at `<tools dir>/<tool>/<version>/<file>` (an absolute path); the tools directory is `CODERIPPER_TOOLS_DIR`, else
    `tools` beside the build cache. Nothing is written to a `PATH` or any system location.
  - `tools.lock` pins, per tool and platform, version, source, SHA-256 and SPDX licence. Names that become path components are
    validated (no separators, no `..`, no Windows reserved names, unique ignoring case); a source is `https://`, `http://` on this
    machine (userinfo and backslashes refused), or an absolute `file://` path (no percent-decoding). An `https://` download may
    redirect but never down to `http://`.
  - Nothing is fetched, written or deleted without `--install-tools`; otherwise the error is `consent_not_given` with the exact
    command (naming the same `--tools-lock` when one was given). A download is hashed before anything is written, so a mismatch is
    `checksum_mismatch` and leaves no file. An installed file that no longer matches its lock entry is reported and left alone
    without consent, and replaced with it. Other codes: `tool_missing`, `tool_unavailable_for_platform`,
    `download_failed`, `tool_install_failed`, `tools_lock_invalid`.
  - New dependencies: `sha2`, and `ureq` (rustls) behind the `cli` feature for the download.

### Known limits (deliberate for this PR)
- The shipped lock is empty: no tool is pinned until the delegation PR that needs it. The installer handles a single-file binary
  only; archives, `cargo install`, `npm` and `pip` installers come with the first tool that needs them.
- There is no interactive prompt and no user configuration file yet; `--install-tools` is the only consent. `tools bundle` and
  `tools install --from` (offline sets) are not built; a `file://` source covers a mirror for now.
- `--install-tools` is accepted by `coderipper tools install` only; the run commands do not take it until a delegated rule uses a tool.

## 0.3.6 - 2026-10-09 (allocated by the PM)

### Added
- **The language-neutral module and its first five rules** (multi-language P3, PR 1), behind `--profile extended`: `SUP-001` (a
  committed lockfile for an application; CI installs in the strict mode), `SUP-011` (vendored code excluded from analysis
  configuration), `DOC-005` (decision-record status and supersession links), `DOC-009` (a glossary exists; no term defined twice),
  `WSP-001` (a default code owner). Each reads the tracked files at `HEAD`, reports under its catalog id, honours the
  `.coderipper.toml` allowlist, and is proven by a seeded-defect fixture and a clean twin under `conformance/<rule>/`. Each is a
  narrow reading of a broader rule; the module documents what it does not see (for example, `WSP-001` judges the owner, not an
  inventory with importance and stage of life, and `DOC-009` does not judge how terms are used). The `DOC-009` fixture proves the
  missing-glossary reading; the duplicate-term reading is covered by unit tests only.
- `coderipper::module::NeutralModule`, `coderipper::module::Composite` (modules asked as one, with one allowlist and one rule-id set),
  `coderipper::Profile`, `run_checks_in` and `run_workspace_in` (the existing `run_checks` and `run_workspace` are `Classic`),
  `coverage::for_missing_module_with`.
- `coderipper check <RULE> --profile extended` runs one neutral rule; `coderipper conformance --module neutral` judges them.
- Coverage is derived per rule and language from what each claim really reads. A claim may name its languages
  (`RuleClaim::languages`); `SUP-001` reads Cargo and npm-family manifests only, so a Python or Go project shows it as not yet
  implemented, while the other four (which read file kinds, not languages) are covered for every language. A workspace run takes the
  rules once, at the workspace root.
- Catalog records gain `checked_today`: what the check judges now and what it does not. The five records say so (`WSP-001` judges a
  default owner, not an inventory with importance and stage of life; `DOC-009` a glossary's existence and duplicate terms, not
  usage; `DOC-005` status and supersession links, not references to enforcing rules; `SUP-001` Cargo and npm only).

### Known limits (found by the independent audit; deliberate for this PR)
- A tracked file that cannot be read is skipped by the rule that wanted it, not reported as an error (only a project that is not a
  git repository with a commit is an error for every rule).
- `check <RULE> --profile extended` for a rule that does not apply here (for example `DOC-005` in a repository with no decision
  records) fails the run as "no verdict", because the host treats a module that skips every requested rule as having given none.
- Files are read one `git show` at a time; a project below the repository root sees only its own directory (so a repository-level
  `CODEOWNERS` above it is not seen).

### Changed
- `--profile extended` now also runs these rules, so it can report findings (and, with `--deny`, fail) where it only printed the
  coverage report before. `classic`, the default, is byte-for-byte what it was.

## 0.3.5 - 2026-10-09 (proposed; the PM allocates the number at gate time)

### Added
- **The coverage report and `--profile`** (multi-language design sections 4.4, 6.4, 10.2). `coderipper::coverage` joins the rule
  catalog with what a module claims and has *proven* (a claim with a conformance proof is covered; one without is counted as a
  gap, not as coverage) and with this run's outcomes (clean, findings, skipped, could not run). `--profile classic|extended`
  (default `classic`) and `--coverage[=full]` print it: one line per language, a "Not yet implemented" section, and one
  `coderipper-coverage` JSON line per language before the summary line.
- **A gap never fails a run** (the owner's ruling): coverage is information about CodeRipper's own incomplete implementation, worded
  as such, and no exit code depends on it. Exit codes are exactly as before.
- **One stderr line for a language with no module** under `classic`: `coderipper: typescript: 14 source files found, not
  checked (classic profile; see --profile extended)`, for a manifest plus at least one source file among the files git tracks.
  Stdout and JSON are unchanged, so nothing that reads them changes.
- `RunResult::outcomes` (how each requested rule fared). The built-in Rust module marks the four claims its conformance fixtures
  prove; a test fails if that list and the fixtures disagree.
- No behaviour change under `classic` apart from the stderr line.

## 0.3.4 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **`coderipper::conformance` and `coderipper conformance`**: how a module earns a claim to cover a rule (multi-language design
  section 6.2). Each rule has a seeded-defect project and a clean twin under `conformance/<rule>/`, each with an `expect.toml`
  naming where the rule must report (or, for the twin, nothing). The runner copies each into a throwaway git repository, runs the
  one rule, and calls it `proven` only if every expectation is met, nothing unexplained is reported, and the twin is silent.
  A claim with no complete pair is `no fixture`; one that needs the network is `unproven` unless `--network`. `--require-proven`
  turns anything short of proven into exit 1 (for a CI job).
- Fixtures for the four original checks that can run offline: `reachability`, `unused-parameters`, `unused-return-values`,
  `version-consistency`. `ci-protection-presence` needs a GitHub repository and stays claimed but not earned. A test puts the defect
  into a clean twin and requires the real module to fail it, so the passes are not vacuous.
- No behaviour change for the other commands or `run_checks`.

## 0.3.3 - 2026-10-08 (proposed; the PM allocates the number at gate time)

### Added
- **The whole rule catalog**: `Catalog::builtin()` now holds 221 rules: the 216 of the owner's knowledge base (informed by it, written
  in CodeRipper's own words: statement, rationale, remediation, applicability, related rules; every one `experimental`) and the five
  original checks. Records are in `rules/<domain>.toml` (the concurrency domain is `concurrency.toml`: `con` is a reserved device name
  on Windows).
- **`docs/rules/`**: a guide page per domain (the problem, the practices, the cautions, and a table of where each rule is checked, which
  says "no check yet" for everything that is not implemented), the verification model, the contested-guidance register and language
  profiles, the lessons behind the rules, and a reading list.
- Nothing the command or `run_checks` does has changed: the catalog is read by nothing yet, and no new rule runs.

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
