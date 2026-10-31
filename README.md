# CodeRipper

Audits a Rust project for the integration defects that the compiler and clippy do not report: public functions
nothing calls (`rustc`'s `dead_code` lint deliberately skips `pub` items), return values every caller discards,
parameters nothing reads, crates in one project that disagree on their version, and a repository whose default
branch enforces no CI. Each is a pluggable check under one host that reports in one `Finding` shape.

It is one engine with three faces: **a library**, **a command** (`coderipper`) that is also **a cargo subcommand**
(`cargo coderipper`), and, planned, a hosted service for open-source projects (not part of this crate).

## Run it next to clippy

```sh
cargo install coderipper

cargo coderipper                     # every fast (local-only) check on the project in the current directory
cargo coderipper --deny medium       # ... exit 1 only for Medium or worse (any finding fails by default)
cargo coderipper --deny none         # print findings, never fail on one (the behaviour before 0.5.0)
cargo coderipper --workspace         # every member of the cargo workspace

coderipper fast                      # the same, as a plain command
coderipper sweep                     # everything, including network-backed checks
coderipper check reachability        # one check by id
coderipper check unused-return-values  # is a function's return value ever consumed?
coderipper check unused-parameters   # which function parameters are never used?
coderipper check version-consistency # does every package of the project share one version?
coderipper check ci-protection-presence # does the default branch require status checks? (network; sweep tier)
coderipper check dependency-freshness # is every locked package at its newest version? (network; sweep tier)
```

The project must be a git repository with a commit: the checks analyse `HEAD` in a throwaway worktree and never touch
your working tree.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | every check ran and no finding is at or above `--deny` (the default is `info`: no finding at all, other than a waived one) |
| 1 | a finding at or above `--deny` that no `[[allow]]` entry waives |
| 2 | usage error: an unknown flag or `--deny` level, an unknown check id, or a project path that cannot be read |
| 3 | the audit could not be completed: a check could not run (this outranks 1), or the command itself failed (for example `cache status` with the cache off) |

`--deny <none|info|low|medium|high|critical>` is the severity at which a finding fails the run, and **since 0.5.0 it
defaults to `info`: any finding fails the run.** `--deny medium` lets Low and Info findings through; `--deny none` prints
findings and never fails on one (what every earlier version did). A finding is waived, not failed, when the project says so
on purpose (see "Waiving a finding"). **A coverage gap never fails a run**: a rule CodeRipper has not implemented for a
language, or whose tool is not installed, is reported as not run, and that is CodeRipper's incomplete coverage, not an issue
with the code being checked. A check that could not run is exit 3, as before.

## A repository with no Rust crate at its root

The Rust checks (`reachability`, `unused-return-values`, `unused-parameters`, `version-consistency`) judge a cargo package. Where
the project directory has no tracked `Cargo.toml`, CodeRipper no longer asks cargo about it and fails (exit 3, before 0.6.0):

- **No Rust anywhere in the repository:** the four rules are *not run here* (a coverage gap, never a failure), one `note:` says
  `no Rust crate at this path`, and the rules that are not about Rust (`ci-protection-presence`, and under `--profile extended` the
  language-neutral and delegated rules) run as usual. Exit 0 when nothing else is wrong.
- **Rust crates below the root** (a SvelteKit app whose server is `backend/`, a repository of several independent crates): they are
  found from the git index (never by walking the disk), the run says which (`found 1 Rust crate(s) below it and checked each where it
  is: backend (backend/Cargo.toml)`), and each is checked the way `--project <its directory>` checks a member: its own
  `.coderipper.toml` applies (not the root's), its findings lead with its package name, and `version-consistency` runs once per
  workspace root. A `Cargo.toml` that is on disk but not committed is cargo's error (exit 3), not "no Rust". A nested workspace is checked member by member. `--workspace` does the same
  when the root has no crate. A crate that is found and cannot be built is an error (exit 3), never a skip.

## Reading the text output

A finding prints as `[<severity>/<confidence>] <package or project> — <summary> at <file>:<line> (<check id>)`; a finding about a
whole file has ` at <file>`, one with no place has neither, and every finding line ends in `(<check id>)` (a waived one then adds
`-- reason: <reason>`). A check that could not run prints, on stderr, `coderipper: check error: <check> failed to run: <why>` (or
`failed to run (<kind>): <why>` for a delegated tool); for a failed build the why ends `Cause:` and one line: the first compiler error with its
code and place, or what cargo and a failing build script said. The text is for people; `--message-format json` is the stable interface.

## Machine-readable output

`--message-format json` prints one JSON object per line on stdout, each with a `"reason"`, the way cargo's own
`--message-format json` does: a `coderipper-finding` line per finding (the `Finding` fields), a `coderipper-waived` line per waived finding (the `Finding`
fields plus `waiver_reason`, the entry's reason), then one `coderipper-summary` line with `findings`, `waived`, `errors` and
`exit_code` (`waived` is new in 0.5.0; a reader that ignores unknown keys is unaffected). In this mode stdout carries only JSON.

```sh
cargo coderipper --message-format json --deny medium
```

**`--summary` (0.9.8): one line per rule.** `coderipper sweep --summary` ends the human output with a totals line and one line per rule (findings, `[[allow]]` waivers, `public_api` declarations, severities); with `--message-format json` it prints one `coderipper-rule-summary` line just before the `coderipper-summary` line. The field names are stable: `reason`, `project`, `findings`, `waived`, `declared_public`, `opt_outs` (waived plus declared public: the project's visible opt-outs), `errors`, and `rules`, a list sorted by rule id whose items have `rule`, `findings`, `waived`, `declared_public` and `severities` (lower-case severity to count). A rule that ran clean has no row, and `--summary` changes no exit code.

## As a library

```toml
[dependencies]
coderipper = { version = "0.18", default-features = false }   # no clap, no binaries
```

```rust,no_run
use coderipper::check::{CheckContext, Tier};

let ctx = CheckContext::new("path/to/project");
let result = coderipper::run_checks(&ctx, Tier::Fast, None);
for finding in &result.findings {
    println!("{:?} {} ({})", finding.severity, finding.summary, finding.check_id);
}
assert!(result.errors.is_empty()); // a check that could not run is an error, never a silent absence of findings
```

Write your own check by implementing `coderipper::check::Check` and run it with `coderipper::run_checks_with`, which
gives it the host's validation and the project's allowlist. The `examples/` directory has a runnable example of each:
the built-in checks (`run_on_a_project`), your own check (`custom_check`), and a check that talks to GitHub tested
with no network (`fake_github`). The data types are `#[non_exhaustive]`: build them with their constructors and
match enums with a wildcard arm. `Check` and `Github` are `Send + Sync`. The minimum supported Rust version is 1.89.

**The build cache is the command's, not the library's.** `coderipper` turns it on from `CODERIPPER_CACHE_DIR` and friends (see "Build cache" below). A library
user gets no cache unless they call `coderipper::build_cache::set_cache_config(CacheConfig::new(dir))` once per process (a second call returns `false`), and the
notes the command prints when the cache is busy or unusable are not available to a library; the checks then build uncached, which is correct and slower.

## Status

The host, the shared `Finding` schema, and the check-plugin interface exist, and six checks are implemented:
`reachability` (dead code, including `pub` items), `unused-parameters` (a function parameter never used),
`unused-return-values` (a function whose return value every caller discards), `version-consistency` (one version per
project), `ci-protection-presence` (the default branch requires status checks) and `dependency-freshness` (every package in
`Cargo.lock` is at the newest version the registry has).

## Version consistency

`coderipper check version-consistency` reports every package of a project whose version differs from the
project's (CireSnave's rule: one version per project). Run it with `--project` on the **workspace root** (a
virtual workspace root is fine); from a member's directory it only says so. The project's version is `[workspace.package] version`
when the workspace defines one, otherwise the version most packages are at (a tie goes to the highest). A package
with no `version` key is left out. A package
that exists to work with another project's version is declared in `.coderipper.toml`, and is then compared with
that project's current version instead; the reason is required. `manifest` is a package's manifest (its version
as cargo resolves it, `version.workspace = true` included) or a virtual workspace's manifest (the one version its
packages share; an inconsistent one is an error):

```toml
[[tracks]]
package = "baracuda-unpopped"
manifest = "../unpopped/Cargo.toml"
reason = "emitter for Unpopped; keeps Unpopped's version"
```

A package can also be silenced with an ordinary `[[allow]]` entry (check `version-consistency`, `file` = its
manifest relative to the workspace root, `symbol` = the package name). Rust (cargo) manifests only; it reads the
working tree, not HEAD.

## Members that cannot be built where the run is

`--workspace` builds every member, and a member whose build fails is "not judged" and fails the run. A member that can never build on the runner (it links an SDK the runner lacks) is declared in the repository's own `.coderipper.toml`, and then is not built:

```toml
[[skip_member]]
name = "fuel-cuda-backend"        # the cargo package name
needs = "nvcc"                    # optional: skipped only while this executable is not on PATH
reason = "links the CUDA toolkit; no hosted runner has it"
owner = "fuel lane"
```

`reason` and `owner` are required. Each run names the member, `fuel-cuda-backend: not judged (declared skip): <reason> (owner <owner>; needs `nvcc`, which is not on PATH)`, and counts it in the workspace line; it is not a failure. With `needs`, a runner that has the tool builds and judges the member. An entry that names no package of the workspace is an `allowlist` finding. Only the repository's own file is read for this key, and it hides no finding from a member that is built: a finding is waived with `[[allow]]`.

## A binary whose contract differs: `[[cli_waiver]]`

`OPS-011` holds every binary to the standard flags. A binary whose documented contract differs (a usage error that exits 1, because exit 2 means something else) waives the probes it cannot meet, in the repository's own `.coderipper.toml`; the other probes are still judged:

```toml
[[cli_waiver]]
bin = "lane-restart"              # the binary's name
probes = ["unknown-flag"]         # any of "--version", "-V", "--help", "-h", "unknown-flag"
reason = "usage error exits 1; exit 2 means awaiting confirmation (documented contract)"
```

`bin`, `probes` and a non-blank `reason` are required; an entry without a reason, with no probe or with an unknown probe name is a configuration error. Each run prints the waived probe as a note, `waived: `lane-restart --coderipper-no-such-flag` (...): <reason>`; a waiver whose probe passed, or whose binary was not built, is noted as stale. Only the repository's own file is read for this key.

## CI and branch protection

`coderipper check ci-protection-presence` reads GitHub's branch-protection settings for the repository `origin`
points at (through `gh api`, as whichever account `gh` has active; read-only) and reports a default branch that
requires no status checks: protection missing, or enabled but with `enforcement_level` `off` or zero required
contexts. It never trusts `protected`, which is `true` on branches that enforce nothing. It is a network check, so it
runs under `coderipper sweep` or by name, never in `fast`. It cannot silently pass: no `origin`, a non-GitHub origin, an
API failure, or a reply without the `protection` object (hidden from a token without push access to a private
repository) is an error; an archived repository, a fork and one with no commits yet each get an `Info` note. Since 0.9.27 it
also reports, at `Medium`, each repository setting that differs from the portfolio's (squash-only merges, merged branches
deleted, and on the default branch admins able to bypass, no force-push, no deletion); reading those needs admin on the
repository. Since 0.9.41 a default branch governed by rulesets alone (no classic protection) is also reported, at `Medium`, for each of
force-push and deletion that no active ruleset rule blocks (rule types `non_fast_forward` and `deletion`). To accept a
repository on purpose, use an `[[allow]]` entry with `check = "ci-protection-presence"`,
`file = "github:branch-protection"` and `symbol = "owner/repo@branch"` (the finding is then waived, with your reason shown).

## Workspaces

A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
root that is itself a package. A virtual workspace root is refused with the list of members to choose from.
The whole repository is checked out in a throwaway worktree (so path dependencies resolve) but only the
member is rewritten and built, findings carry paths relative to the member, and `.coderipper.toml` is read
from the member's directory. The checks are project-scope: a `pub` item used only by a sibling member has no
callers within this package. `reachability` is the exception: it also reads the members that depend on this package
(directly or through other members, by `path`) and does not report an item one of them names. Dependencies are built once and reused (see "Build cache" below).

### `--workspace`: every member in one command

`coderipper check reachability --workspace --project fuel` (also `fast` and `sweep`) runs the checks over **every
member** of the cargo workspace containing the project (the root, or any member's directory). Each member is judged
exactly as `--project <member>` would judge it: its own `.coderipper.toml`, the same stale-entry reporting. Checks that
judge the whole repository (`version-consistency`, `ci-protection-presence`) run once, at the workspace root; a
`[workspace] exclude`d directory is not a member. Members run in sorted order; stderr shows
`coderipper: member <name> (i/n)` as each starts and ends with a one-line summary. Every finding leads with its
**package name** (two members can share a directory name).

All members share one checkout of the repository for the run: each member is rewritten in place, built, and put back
with `git checkout`, and a sibling nobody touched is compiled once and reused by the next member (about the same
work as one cold workspace build plus each member's own compile, instead of a full rebuild per member and check). While
a `--workspace` run is in progress it holds the build cache's lock for that repository, so another coderipper run on the
same repository builds without the cache and says so. An error in one member (it does not compile) is reported by member and
the others still run; the process exits non-zero. If the shared checkout cannot be put back to a clean state the run stops
and says which members were not analysed.

## Many repositories in one command: `--scope`

`coderipper sweep --scope portfolio.list` (also `fast` and `check ID`) runs the command over every repository a file lists, instead of one `--project`. The file has one path per line; `#` starts a comment, blank lines are ignored, a relative path is read from the directory of the file, and a repeated path runs once. Each repository is analysed on its own, under its own `.coderipper.toml`, exactly as `--project` would; `--workspace` and the other options apply to each. The output is grouped: `== scope: <repository> (i of n) ==` before each (a `coderipper-scope` line with `repository`, `index` and `of` under `--message-format json`), and a closing line, `coderipper: scope: N repositories, C clean, F with findings, X could not be run` (a `coderipper-scope-summary` line in JSON, with `exit_code`). A repository that cannot be read or run is reported on stderr and counted as could-not-run; the others still run. The exit code is the worst of them (3 over 1 over 0); a list that names no repository, or cannot be read, is a usage error (2), and `--scope` cannot be combined with `--project`. Used with the result cache (below), a weekly sweep of an unchanged portfolio re-analyses only the repositories whose committed tree changed.

## Build cache

The checks that compile your package build it in a throwaway checkout. Without a cache every run recompiles every
dependency (about two minutes for a large crate). CodeRipper therefore keeps one persistent target directory per
repository and toolchain, outside your project, and points cargo at it: a later run, from a *different* throwaway
checkout, finds the registry, git and out-of-repo path dependencies already built. Measured on `fuel-core` (336
packages): 234 s cold, 90 s with the cache. What is **not** reused: a path dependency *inside* the repository (a
sibling workspace member) is rebuilt in every run, because its freshly checked-out files are newer than the cached
build, and the package being analysed is always rebuilt.

- **Where:** `CODERIPPER_CACHE_DIR`, else `%LOCALAPPDATA%\coderipper\build`, else `$XDG_CACHE_HOME/coderipper/build`,
  else `~/.cache/coderipper/build`. Never inside a project or its `target/`.
- **Off:** `CODERIPPER_CACHE=off`. Correctness never depends on the cache: if it cannot be used the build runs
  uncached, exactly as before, and a note says why.
- **No hangs:** CodeRipper takes its own lock on the repository's cache directory first and waits at most
  `CODERIPPER_CACHE_WAIT_SECS` (5), then builds uncached and names the last holder's pid.
- **Size:** `CODERIPPER_CACHE_MAX_GB` (20). The first build of a run deletes least-recently-used repository
  directories until the cache fits (never a locked one, never the one in use, never in a directory without the
  `.coderipper-cache` marker). `coderipper cache status` lists them; `coderipper cache prune [--max-gb N]` trims.
- Each run that built something ends with one stderr line: `coderipper: cache <dir> - N units fresh, M compiled`.

**The result cache (the incremental cache).** The checks that build the project (`reachability`, `unused-return-values` and `unused-parameters`) analyse the committed tree, so their result is a function of it. When the build cache is on, a run stores each such check's result under `<cache root>/results/`, and a later run over the same inputs reads it back instead of building: this repository's `reachability` went from 48 s to 1 s on an unchanged tree. The key is the SHA-256 of CodeRipper's version, the check, the project's path in its repository, the git tree hash of `HEAD` (the whole tree, on purpose: a build can read any file in it), `rustc -vV`, the cargo features and the build environment variables (`RUSTFLAGS` and the like); a change to any is a miss. A run that reused or computed a result through the store ends with one stderr line (`coderipper: result cache - N reused (same committed tree), M computed`); it is not a note on the result, so what a clean run prints is unchanged. Correctness never depends on it: a check that ended in an error is never stored, and a damaged entry, a repository that is not git or a failed write just runs the check. `CODERIPPER_RESULT_CACHE=off` turns it off alone (`CODERIPPER_CACHE=off` turns off both); 4000 entries are kept. A change to one workspace member re-analyses every member of that repository (the key is the whole tree); a member-level key needs the set of files a build reads, which is on the roadmap.

## Reachability on a package with a library

`reachability` builds only the library when there is one, then does not report an item that a bin, test,
example or bench reaches by name (and everything that item reaches). It can therefore miss a dead item that
shares a name with something live, but it does not report live code as dead. `pub` items inside the bins
themselves are not analyzed when a library exists.

## Earning a claim: `coderipper conformance`

A check that says it covers a rule has to show it. For each rule there is a small project with a seeded defect and a clean twin
(`conformance/<rule>/defective` and `clean`, each with an `expect.toml` saying where the rule must report, or that it must report
nothing). `coderipper conformance [--rule ID] [--fixtures DIR]` runs them and prints, per rule, `proven`, `FAILED` (with why),
`unproven` (needs the network; add `--network`) or `no fixture`. It exits 1 when a fixture contradicts a claim, 3 when a rule
could not run on its fixture; add `--require-proven` to fail on anything short of proven. Fixtures are code that runs (the checks build
them), so use only fixtures you trust. This is for the people who write checks and modules, not for a normal run.

## Coverage: what CodeRipper does and does not check yet

`--profile extended` (or `--coverage`, or `--coverage=full` to list the ids) prints, per language, how many of the catalogued rules
CodeRipper really covers (a claim counts only when a conformance fixture proves it), how many it does not cover yet, and how this
run went. A rule CodeRipper has not built is **not yet implemented**: that is CodeRipper's own gap, not a finding about your code,
and it never changes the exit code. A language in your repository that has no module is listed the same way. The `classic`
profile (the default before 0.9.0) prints exactly what it always has, plus one stderr line when it finds a language it has no module for (a manifest and at
least one source file among the files git tracks; a project outside git is not scanned).

**The percentage of rules passed (every run, every profile).** Two different numbers, not to be confused. *Coverage* is how much of
the catalogue CodeRipper can check at all (its own completeness). *Rules passed* is, of the rules that **ran to a verdict** in this
run, the share that found nothing: `clean / (clean + with findings)`. A rule that did not apply here, or could not run (a missing tool),
is in neither count, so a gap neither raises nor lowers it; a rule counts once however many findings it has, so it is a metric for
judging improvement over time, not a severity measure. The human output ends with `coderipper: rules passed: 7 of 9 that ran (77.8%)` (on stdout; under `classic`, whose stdout is unchanged, on stderr);
`--message-format json` adds `rules_ran`, `rules_passed` and `rules_passed_percent` (`null` when no rule ran) to the summary line, and the
coverage line's `run` object carries `passed_percent`. It never changes the exit code. An optional floor on *coverage* (not on rules passed), `--min-coverage <pct>`, exits 1 when the covered percentage of any language reported is below `pct`
(a language in the repository that no module checks is measured by the language-neutral rules alone), prints which on stderr, and turns the coverage report on; it never lowers a worse exit code (3 over 1 over 0).

## Pull-request check: `--profile portfolio`

The shared workflow (`portfolio.yml`) sweeps a cargo workspace member by member (`--workspace`, whenever the root `Cargo.toml` has a `[workspace]` table); a repository whose root is a package with members, or a virtual workspace, therefore sees findings in its members that a root-only run did not show. On a GitHub-hosted runner it keeps CodeRipper's build cache between runs.

`--profile portfolio` is the profile a repository's pull-request check runs: the five original checks and every language-neutral rule above, **without** the rules delegated to a tool (no network, nothing to install). It fails the run by a floor per rule instead of one `--deny`: a mechanical violation blocks (`SUP-004` and `version-consistency` at Medium, `conflict-marker` at Low, `patent-index-match` at High, which is the index's `avoid` action), and every other rule reports without blocking until the owner rules it blocking. `--deny none` still never fails on a finding, and a waiver is an `[[allow]]` entry as everywhere. The floors are `coderipper::portfolio_floor`. A repository that is clean for an advisory rule makes it blocking for itself with `blocking = ["SUP-001", "WSP-001"]` in its own `.coderipper.toml` (rule ids; a finding of such a rule at Low or above then fails the run; it never lowers a default floor, and a name that is no rule is a usage error).

## Language-neutral rules: `--profile extended`

Under `--profile extended` five rules that read a repository's files, not its source (and one that hands the repository to a tool,
see below) run next to the original checks, in the same
run and under the same `.coderipper.toml` allowlist. Their ids are catalog ids; each is a narrow, deterministic reading of a broader
rule, and each is proven by a conformance fixture (`coderipper conformance --module neutral`). Since 0.9.0 the default profile (`extended`) runs
them; `--profile classic` does not. `coderipper check SUP-001 --profile extended` runs one.

| Rule | Reports |
|---|---|
| `SUP-001` | an application (a Cargo package with a binary, or a private npm package) whose lockfile is not committed; a GitHub Actions workflow that runs `npm install`, `yarn install` or `cargo build`/`test`/... without the strict mode (`npm ci`, `--frozen-lockfile`/`--immutable`, `--locked`) |
| `SUP-011` | a vendored directory (`vendor`, `third_party`, `node_modules`, ...) that a tracked analysis configuration (`codecov.yml`, `.codacy.yml`, `sonar-project.properties`, `.eslintignore`, `.deepsource.toml`) does not mention |
| `DOC-005` | a decision record (markdown in an `adr`, `adrs` or `decisions` directory) with no or an unknown status, or a supersession link that dangles, is not reciprocated, or leaves the old record current |
| `DOC-009` | a repository with ten or more `.md` pages under `docs/` or `doc/` but no glossary file (fewer pages are "not applicable"); a glossary that defines one term twice |
| `SUP-004` | a Cargo or npm dependency that is not a published version: a `path` (or `file:`/`link:`) dependency that leaves the repository, a `git` dependency or an npm git/URL spec (waivable with a reason), and a Rust `include!`/`include_str!`/`include_bytes!`/`#[path]`/`CARGO_MANIFEST_DIR` join that leaves its crate |
| `patent-index-match` | a declared dependency (`Cargo.toml`, `package.json`, `pyproject.toml`, `requirements*.txt`, `pom.xml`, `build.gradle[.kts]`) that the pinned [agent-safety-bed](https://github.com/ThinkersJournal/agent-safety-bed) patent index lists by registry name, reported by the entry's action: `avoid` High, `flag-for-review` Medium, `allow-with-conditions` and `separate-component` Low, `allow` not reported. Each finding carries the entry's id, tier and note and says **index match**: it is due diligence, not legal advice or a patent clearance, and a dependency with no match is not cleared. The index copy is pinned in `data/` (version and commit in `data/patent-tiers.PIN`), so no network is used. Lockfiles (`Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `poetry.lock`, `uv.lock`, `Pipfile.lock`, `gradle.lockfile`) and Gradle version catalogs are read; a package only a lockfile names is one step lower. Not judged: `go.sum` (the index lists no Go module, so it could match nothing), a technique written in the project's own source. Waivable with an `[[allow]]` entry (file = the manifest, symbol = the package) |
| `licence` | no licence file at the repository root (`LICENSE*`, `LICENCE*`, `COPYING*`, `UNLICENSE*`, `LICENSES/`) is High and blocks a `--profile portfolio` run; reported and never blocking: a manifest `license` identifier no licence file is named for, source files with no `SPDX-License-Identifier` in their first 15 lines, and a header naming another licence with no `NOTICE`/`THIRD-PARTY` origin record; a copyright line naming the owner as a person is Info only (CireSnave's standing ruling, 2026-10-25: "All licenses/copyrights can name me"). Not judged: the licence text, manifests below the root, markup and data files |
| `dependency-freshness` | a package in `Cargo.lock` with a newer version on the registry (network; sweep tier): a newer compatible version, or a release a manifest requirement excludes, from `cargo update --dry-run --verbose`; Medium for a dependency a workspace member declares, Low for a transitive one, waived by `[[allow]]` (`file = "Cargo.lock"`, `symbol` = package). Cargo only; other ecosystems' lockfiles are not read |
| `ci-security` | a workflow file that gives a pull request from outside more than it should (advisory in `--profile portfolio`; meant for public repositories): a `pull_request`/`pull_request_target` job that reads a secret other than `GITHUB_TOKEN` (also `secrets: inherit`, or a top-level `env` secret) High; a `pull_request_target` job that checks out the pull request head High; a job on a self-hosted or custom-labelled runner that mentions no GPU/CUDA/Vulkan/ROCm/OpenCL Medium; a self-hosted job under `pull_request`/`pull_request_target` without the same-repo `if:` (`github.event.pull_request.head.repo.full_name == github.repository`) High, or with it but no `environment:` Medium (waive on a private Free-plan repository); a `pull_request` trigger whose `branches:` filter names neither main nor master (or whose `branches-ignore:` names one) Medium; a workflow with no top-level `permissions:` Low. Not judged: visibility, the forge's approval setting, expression-chosen runners, secrets inside composite actions |
| `version-and-changelog` | on a branch (a run with a base: the merge-base of `HEAD` with `origin/HEAD`, `origin/main` or `origin/master`; needs the history, so `fetch-depth: 0` in CI): the root `Cargo.toml` or `package.json` version is the one the base had, or a version that changed has no entry in the root `CHANGELOG*` (a heading or leading line, a list item included, naming it as a whole token: `0.8.1` is not an entry for `0.8.12`), both Medium and advisory in `--profile portfolio`. A coverage gap on the default branch, with no origin branch, or in a shallow clone. Not judged: the `version-deferred` label, manifests below the root, `pyproject.toml`, what an entry says |
| `CON-006` | Rust, from the source (`syn`): a lock is named by the path that reaches it (`self.field`, `self.a.b`, `self.shards[i]`, a `static`), and a guard held while another lock is acquired is an edge (a `let`-bound guard to the end of its block or `drop`, a temporary to the end of its statement; a plain `if` condition ends before the body, an `if let` scrutinee and a `match` scrutinee live through it). A **cycle** among the edges of the repository (`a -> b` in one function, `b -> a` in another) and a lock taken while it is already held are reported at **High** severity and **Low** confidence: a possible deadlock, read from direct nesting only. Not seen: a lock reached through a local variable or a parameter, an edge made by a call, a guard moved into a function or closure, macros, tests, benches and examples. A pass is evidence about the nesting `syn` can see, no more. |
| `OPS-011` | Rust binaries (default on in `extended` and `portfolio`, not `classic`; builds the package's binaries from `HEAD` and runs them, so it executes project code): each binary built from the committed tree is run through `--version`, `-V`, `--help`, `-h` and an unknown flag, each in a fresh temporary directory with an empty environment (only `PATH` and the Windows system variables) and a 10-second timeout. It expects exit 0 and a version line containing the manifest version, usage on stdout, exit 2 with the message on stderr and stdout empty for the unknown flag, no ANSI escape sequence in any output, and nothing created or changed under the working directory or the directories `HOME`, `APPDATA`, `LOCALAPPDATA`, the `XDG_*` roots and the temp directory point at. A probe that times out is run again, and makes the finding Medium; any other failure is Low. Allowlist a binary that cannot answer (subject = its name) with the reason. Rust only: `package.json` `bin` entries and Python console scripts are NOT CHECKED. |
| `OPS-012` | Rust binaries, from the source (`syn`; `portfolio` and `extended`): a fixed editor (`nano`, `vim`, ...) in a file that never names `EDITOR`/`VISUAL`; a dotfile joined onto the home directory in a file that never names XDG, `APPDATA`, `config_dir` or a directories crate; `env::args` with two or more flags and no argument-parser dependency in `Cargo.toml`; `-v` printing the version or `-V` setting verbosity; and, only for a feature the binary offers, `--yes`/`--quiet`/`--verbose` without `-y`/`-q`/`-v`, stdin read with no `-` operand, a colour crate that ignores `NO_COLOR`, a never-ending loop or listener with no Ctrl-C handling. Low severity; Medium confidence (Low for the stdin and Ctrl-C cases). Not judged: other languages, a name in another file, `format!`-built paths, a run of the binary |
| `security-support-statement` | a repository whose root manifest declares a version that could be published (as `patent-sweep-record`) and that tracks a `SECURITY.md` (root, `.github/` or `docs/`) with a section headed "supported versions": a table row marked with a check mark for a line older than the manifest's, or two marked rows, or a support period stated as a number of months or years (digits, or one to twelve, eighteen, twenty-four as words). Medium; reported, never gating (experimental). A repository with no `SECURITY.md` or no such section is not judged. A project that really supports more than one line, or a period, opts out with an `[[allow]]` entry and a reason. |
| `patent-sweep-record` | a repository whose root `Cargo.toml` (or `package.json`) declares a version that could be published (not `publish = false`, not `"private": true`) and that tracks no `docs/PATENT-SWEEP.md`, or whose record is not a sweep (no heading for ideas, queries and hits: **a file that exists is not a sweep**), has a hit whose verdict is `unread` (an unread hit is not clear), or names no version in the manifest's version line (`0.9` for any `0.9.x` before 1.0, the major after). Medium; a due-diligence record, not a legal opinion (CireSnave, 2026-10-25: "Ensure a patent sweep has been done of every one of our projects before release."). A project that will never be released opts out with an `[[allow]]` entry and a reason |
| `WSP-001` | a repository with no `CODEOWNERS` file, or one with no default (`*`) owner |
| `conflict-marker` | a tracked text file with an unresolved merge conflict: a line starting with seven `<` and a space opens it, a line starting with seven `>` and a space closes it (a line of seven `=` counts only between them, because alone it is a Markdown heading underline). A fenced block in a Markdown file, and anything under `fixtures`, `testdata`, `conformance`, `snapshots`, `golden` or a vendored directory, is not read. **Every profile, `classic` included** |
| `lint-suppression` | a Rust `#[allow(..)]` (also inside `cfg_attr`) or `#[ignore]` with no reason; `dead_code` is named, because the cure is to delete, use or gate the code. `allow(.., reason = "..")` and `#[ignore = ".."]` pass, and the run prints how many carry a reason. **The ratchet:** a branch whose tree has more `allow` (or more `ignore`) attributes than the merge-base has is a Medium finding at the first surplus one (totals are compared, so a moved file adds nothing; no base is a note, never a pass). Rust only; `#[expect]` and macro bodies are not read. Extended profile only |

Each rule's reading, and what it cannot see, is in the documentation of its module (`src/module/neutral/`). A rule that does not
apply to a repository (no decision records, no manifest it knows) is reported as not applicable here, never as clean. Files are read
from `HEAD`, so the repository must be a git repository with a commit.

### Rules run by a tool: `SEC-002`

`SEC-002` (no committed secrets) is run by [gitleaks](https://github.com/gitleaks/gitleaks) (MIT, an external process, never linked).
It scans the repository's **whole history**, so a secret that was committed and later deleted is still reported: it stays readable to
anyone with a clone. Secrets are redacted by gitleaks itself, so CodeRipper never reads a secret's value and cannot print it. It sees
gitleaks' own patterns at the pinned version and nothing outside git.

```sh
coderipper tools install gitleaks --install-tools     # once; or pass --install-tools to the run itself
coderipper check SEC-002 --profile extended --deny high
```

The repository cannot silence the rule: a committed `.gitleaksignore`, `.gitleaks.toml` or `gitleaks:allow` comment is not honoured (the
project's own way to suppress a finding is the `.coderipper.toml` allowlist). A project in a subdirectory gets the leaks under it, with
paths relative to it; a path that is not a git repository with a commit is an error, not a clean run.

A shallow clone (CI's default checkout) holds only the commits it fetched, so a scan that finds nothing there is reported as not run
(`git fetch --unshallow`), not as clean; what it does find is reported.

A tool that is not there is a **coverage gap, never a failure**: without it the run says `coderipper: note: SEC-002 not run: ...` with the
install command, the rule counts as skipped in the coverage line, and the exit code is not affected. (A tool that was asked for and
could not be had or trusted, a checksum mismatch say, is an error.) Nothing is downloaded unless you pass `--install-tools`.

### Rules run by a tool, over the network: `SUP-002` and `SEC-006`

`SUP-002` (dependencies free of known advisories) and `SEC-006` (no known exploitable dependency vulnerabilities) are run by
[osv-scanner](https://github.com/google/osv-scanner) (Apache-2.0, an external process, never linked). It reads the lockfiles,
manifests and SBOMs it knows anywhere under the project (not inside a `node_modules` directory), resolves each package to an exact version and asks [osv.dev](https://osv.dev)
which advisories cover it. Both rules use the network, so they are **sweep-tier**: `coderipper sweep --profile extended` runs them, and
`coderipper check SUP-002 --profile extended` runs one with the network permitted; a `fast` run leaves them alone.

```sh
coderipper tools install osv-scanner --install-tools     # once; or pass --install-tools to the run itself
coderipper sweep --profile extended --deny high
```

One finding per advisory group of one package in one lockfile, at the lockfile; severity follows the highest CVSS score (an unscored
advisory is `High`). `SEC-006` reports only the groups scored 7.0 or higher (or unscored) and **does not know whether the flawed code is
reachable**: call analysis builds the project's code, so it is not run, and every `SEC-006` finding says so. The answer is as of the day
it was asked: the database moves, and a clean result today can be a finding tomorrow.

The repository cannot silence these rules: a committed `osv-scanner.toml` (its `IgnoredVulns`) and a `.gitignore` entry for a lockfile are
not honoured (the project's own way to suppress a finding is the `.coderipper.toml` allowlist). A project osv-scanner finds no package
sources in is reported as **not run**, not clean; a lockfile it cannot read is an error (exit 3), not a clean result.

### Rule run by a tool, over the network: `DOC-010`

`DOC-010` (documentation links resolve) is run by [lychee](https://github.com/lycheeverse/lychee) (Apache-2.0 OR MIT, an external process,
never linked). It checks every link in the project's tracked Markdown, HTML and text documents (not those below `node_modules`, `vendor`,
`third_party` or `target`): a link to a file must exist, and a web page must not answer 404 or 410. It requests web pages, so it is
**sweep-tier** like the advisory rules above.

```sh
coderipper tools install lychee --install-tools     # once; or pass --install-tools to the run itself
coderipper check DOC-010 --profile extended
```

One finding per dead link, at the document and line it is on. A root-relative link (`/branding/a.png`) is read against the repository
root in a Markdown document, as GitHub reads it; in an HTML page it names the web root of whatever serves the page (`static/` in a
SvelteKit app, `public/` in an Astro site), which the repository does not say, so a link that the page's own line shows written
root-relative is **not judged** and the run says so (a form the reader does not recognise is still read against the repository root). A link lychee could not settle (a timeout, a failed connection, a rate
limit, a `403` from a site that turns bots away) is **not judged**: it is neither a finding nor evidence the link works, and a run where
that is the case is reported as **not run**, never clean (as is a project with no links, or a document lychee skipped). Not checked:
`#fragments`, mail addresses and other schemes. The repository cannot silence the rule: a committed `lychee.toml`, `.lycheeignore` or
`.gitignore` entry is not honoured (use the `.coderipper.toml` allowlist).

### Rule run by a tool: `SUP-008`

`SUP-008` (CI workflows pinned and least-privileged) is run by [zizmor](https://github.com/zizmorcore/zizmor) (MIT, an external process,
never linked). It audits the project's tracked GitHub Actions workflows (`.github/workflows/*.yml`) and action definitions (`action.yml`),
**offline**, and reports two things: an action, reusable workflow or image referenced by a tag or branch instead of an immutable hash or digest, and a
workflow or job whose token permissions are the broad default (no `permissions:` block), `write-all` or `read-all`, or a workflow-level write scope
(a write scope declared on one job is not reported). Nothing else zizmor checks is
reported as this rule.

```sh
coderipper tools install zizmor --install-tools     # once; or pass --install-tools to the run itself
coderipper check SUP-008 --profile extended
```

One finding per zizmor finding, at the file and line it is on, with zizmor's severity and confidence. zizmor cannot know what a job needs,
so a workflow-level `contents: write` that is needed is reported too; say so in the `.coderipper.toml` allowlist. A workflow zizmor could not read (a YAML
syntax error: it skips the file and still exits 0) makes a clean result **not run**, never clean, as does a project with no workflow. The
repository cannot silence the rule: a committed `zizmor.yml` and `# zizmor: ignore` comments are not honoured (a finding that carries such a
comment is reported and says so).

### Rule run by a tool: `API-001`

`API-001` (breaking public changes since the last release) is run by [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks) (MIT OR Apache-2.0, an external process, never linked), from the standalone release build the lock pins (no Rust tool needs installing besides the toolchain you already build with).

```sh
coderipper tools install cargo-semver-checks --install-tools     # once; or pass --install-tools to the run itself
coderipper check API-001 --profile extended
```

It compares the crate (every library of the workspace when the root `Cargo.toml` has a `[workspace]` table) with a **baseline**: the newest release tag below `HEAD` (`v1.2.3` or `1.2.3`; a pre-release tag is not a release), else the merge-base of `HEAD` with `origin/HEAD`, `origin/main` or `origin/master`. It builds the public interface of both sides (so the project's build scripts and procedural macros run, and cargo may download dependencies), runs the tool's lints, and compares the verdict with your version bump: a removed function, struct, enum variant or module, a changed signature and the like, released without the major version (before 1.0, the second number), is a **High** finding at the place the item was in the baseline (a removed item has no place now, and the finding says so). The same change released as a breaking one passes. It runs in a scratch directory with `CARGO_TARGET_DIR` inside it, so nothing is written into your project. No release tag and no default branch, a project with no library target, a missing tool, a platform the lock pins no build for: a reported gap, never a pass. A report that cannot be read is an error, never "no findings".

### Rule run by a tool: `API-007`

`API-007` (database migrations are safe step by step) is run by [squawk](https://github.com/sbdchd/squawk) (Apache-2.0, an external process, never linked), a linter for PostgreSQL migrations, from the single-file release build the lock pins.

```sh
coderipper tools install squawk --install-tools     # once; or pass --install-tools to the run itself
coderipper check API-007 --profile extended
```

It reads the project's **tracked** migrations (a `.sql` file below a `migrations`, `migration` or `migrate` directory, or named like a versioned migration: `V12__name.sql`, `20260101120000_name.sql`) and reports what takes a table offline or loses data while a service runs: a dropped column or table, a changed column type, a renamed column or table, a new `NOT NULL` column without a default, a foreign key or unique constraint added without `NOT VALID`, an index built without `CONCURRENTLY`, and the like. The data-losing or code-breaking ones are **High**, the locking and transaction advice **Medium**. squawk's **style advice** (no `IF NOT EXISTS`, a 32-bit integer, no lock timeout, ...) is not reported as this rule. squawk reads PostgreSQL: a statement it cannot parse is **not judged**, so a clean result beside one is a reported gap and a finding says so; a project with no migration is a gap too. Its configuration file in your repository is not read (it runs in a scratch directory with an empty configuration), **but a `-- squawk-ignore <rule>` comment in a migration is honoured by the tool and cannot be switched off**: every finding of a run that has such comments says how many there are. Suppress a finding in `.coderipper.toml`, where it is visible.

### Rule run by a tool: `API-006`

`API-006` (schema changes obey evolution rules) is run by [buf](https://github.com/bufbuild/buf) (Apache-2.0, an external process, never
linked): `buf breaking` over the project's Protocol Buffers schemas, in buf's `FILE` category (retyping or renumbering a field, removing a
field, message, enum value or file, moving a message to another package, ...). It compares the **working tree** with a **baseline**: the
schema as a git commit has it. The baseline is the ref named by `[buf] baseline` in `.coderipper.toml` (normally the last release), else
the merge-base of `HEAD` with `origin/HEAD`, the point the branch left the default branch at.

```toml
# .coderipper.toml
[buf]
baseline = "v1.4.0"      # a branch, tag or commit
```

```sh
coderipper tools install buf --install-tools     # once; or pass --install-tools to the run itself
coderipper check API-006 --profile extended
```

**When nothing could be compared the rule is not run, and never clean**: no `origin` and no named ref, a shallow clone without the
merge-base, a run on the default branch with nothing changed (the baseline is then the tree's own commit, so buf would compare the schemas
with themselves), a branch that changes no `.proto` file, changed `.proto` files that are not below a module with schema on both sides, a
baseline that holds no `.proto` file (a schema that is new cannot break anyone), and a tree whose every schema was removed. A module
that had schema in the baseline and has none now cannot be compared by buf: the result says so and a clean result is not run. A ref named in
`.coderipper.toml` that does not exist is an **error**. A `.proto` file buf cannot compile (a missing import, a Buf Schema Registry
dependency, a syntax error) stops buf from judging any file (it builds all of the schemas or none), so the rule is **not run**, naming the file. Where the schemas' roots are is read from
the tracked `buf.yaml` and `buf.work.yaml` files (their directory, a v2 file's `modules:` `path:`s, a work file's `directories:`), else the
project's directory. The repository cannot silence the rule: its `buf.yaml` (at the tree and at the baseline) is replaced by a configuration
given on the command line. One finding per violation, at the file and line buf prints, with buf's rule id as the subject.

### Rule run by a tool: `SEC-009`

`SEC-009` (least privilege in infrastructure definitions) is run by [checkov](https://github.com/bridgecrewio/checkov) (Apache-2.0, an
external process, never linked). CodeRipper installs the standalone single-file build that checkov's releases ship (a pinned,
hash-verified download; no Python is needed), only with your say-so. **Which tools CodeRipper runs does not depend on the language they
are written in** (CireSnave, 2026-10-25): a good tool is supported whatever it is written in. The 2026-10-09 decision not to
require Python is withdrawn; checkov happens to ship a binary, and a tool that ships only as a Python package will be offered through
an isolated, version- and hash-pinned environment, never silently and never into the system's Python (not built yet; see ROADMAP).

```sh
coderipper tools install checkov --install-tools     # once; or pass --install-tools to the run itself
coderipper check SEC-009 --profile extended
```

It judges the project's **tracked** Terraform, Dockerfiles, and the YAML or JSON files of Kubernetes, CloudFormation, ARM, Bicep and
Serverless definitions, offline, and runs only the checks that are this rule's wording: wildcard permissions and administrator
grants (`*` actions, `AdministratorAccess`, privilege escalation) and elevated container privileges (privileged containers, shared
host namespaces, added Linux capabilities, a container that runs as root). The rest of what checkov knows (encryption, logging,
versioning, network exposure) is not reported as this rule. A wildcard grant, an administrator policy, a privileged container, a shared
host namespace or a privilege escalation is `High`; running as root or adding a capability is `Medium`. checkov's releases have x86_64 builds for Windows, Linux and macOS only, so on an Apple-silicon Mac (or any other platform) the rule is a reported gap (`tool_unavailable_for_platform`), never a failure. A file checkov could not parse
makes a clean result **not run**, never clean, as does a project with no infrastructure definition. The repository cannot silence the
rule: a `#checkov:skip` comment (or the `checkov.io/skip` annotation) is not honoured, and the finding says so.

## Tools CodeRipper may install: `coderipper tools`

Some rules hand their work to a tool that already does it well (a secret scanner, a link checker). Those tools are never
installed behind your back. They live in an isolated cache, `tools` beside the build cache (or `CODERIPPER_TOOLS_DIR`), at
`<tool>/<version>/<file>`, and nothing is written to your `PATH` or any system location. A `tools.lock` pins, per tool and
platform, one version, its source, its SHA-256 and its SPDX licence; there is no "latest". A download that does not match its
checksum is discarded and reported as `checksum_mismatch`. Without `--install-tools` nothing is downloaded, written or deleted, in CI or anywhere else:
the error is `consent_not_given` and it prints the command that would install the tool.

```sh
coderipper tools list                          # the ledger: version, licence, source, installed or not; installs nothing
coderipper tools install gitleaks --install-tools
```

The lock pins gitleaks 8.30.1, osv-scanner 2.6.0, lychee 0.24.2, zizmor 1.30.1, buf 1.73.0, checkov 3.3.30, cargo-semver-checks 0.51.0 and squawk 2.70.0 for now (`tools list` shows them); each delegated rule adds its tool to the lock when it lands. A tool is installed from a checksummed single-file binary (or a `file://` copy, which
is how an offline mirror works) or from a `.tar.gz` or `.zip` that holds it. For an archive the lock names the one `member` to take
out and pins two hashes: the archive's and the member's. CodeRipper unpacks the archive itself, in memory, and refuses the **whole**
archive (`archive_refused`) if any entry has a name that could leave the directory (`..`, an absolute path, a drive, a backslash) or is
a link, device or other special file, even when the member it wants is fine; only the member is ever written. `cargo install`, `npm`
and `pip` installers are not built yet.

A name that `tools install` is given and the lock does not have is a usage error (exit 2, `tool_missing`), and nothing is installed,
even for the valid names beside it. A tool that exists in the lock but not for this platform is `tool_unavailable_for_platform`
(exit 3).

### Dependencies written in C or assembly

The default build (the `cli` feature) downloads over HTTPS through `ureq` and `rustls`, and `rustls` takes its cryptography from
[`ring`](https://crates.io/crates/ring), which contains C and assembly. That is the only such dependency; the archive readers
(`flate2` with its Rust backend, `tar`, `zip`) are pure Rust. Building without the `cli` feature (`default-features = false`)
leaves `ring` out. Replacing `ring` with a pure-Rust provider is wanted and not done yet.

## Conformance fixtures are not reported

A repository that holds its own conformance fixtures (`conformance/<rule>/<variant>/`, each with an `expect.toml`) is not
reported for what is in them: a fixture is defective on purpose. The run prints one note with the count. A file of the same
shape anywhere else is reported as usual.

## Leaving a check out of a run

`--skip-rule ID=WHY` (repeatable) does not call a built-in check and prints `ID not run: skipped on request (WHY)`. It is for a
check that cannot run where the run is (`ci-protection-presence` needs repository-admin read, which a CI job's own token lacks),
with the place it does run named in `WHY`. A name that is no built-in check and a skip with no reason are errors. It is not a way
to silence a finding: that is an `[[allow]]` entry.

## Waiving a finding

Since 0.5.0 a finding fails the run, so a deliberate one (public API built ahead of its consumer, a value discarded on
purpose) is **waived** in `.coderipper.toml` at the project root. Every entry names the check and the file, and **must**
say why:

```toml
[[allow]]
check = "reachability"
file = "src/api.rs"
reason = "published crate API, consumed outside this repo"

[[allow]]
check = "unused-return-values"
file = "src/main.rs"
symbol = "f"          # optional: only the finding about this symbol
lines = "40-52"       # optional: only findings reported at these lines ("12" or "10-20", inclusive)
reason = "the result is deliberately dropped; see issue 7"
```

### Declaring a library's public API (`[reachability] public_api`)

A published library's API has no consumer inside its own repository, so `reachability` reports it. Rather than one `[[allow]]` per
file, declare the surface once; every entry needs a `reason`:

```toml
[reachability]
public_api = [
  { file = "src/api/**", reason = "published API, consumed by downstream crates" },
  { file = "src/codec.rs", symbol = "decode", reason = "documented entry point" },
]
```

`file` is a path or a glob (`*` and `?` stay inside one directory, `**` crosses directories); `symbol` narrows an entry to one item.
A matched finding does not fail the run and is **not hidden**: it is printed as `[declared-public/<severity>] ... -- reason: <reason>`
(in JSON, a `waived` item with `"declared_public": true`). An entry that matches nothing in a run where `reachability` completed is
reported as an `allowlist` finding saying the path or symbol may be gone. A wide configuration file may hold the key, with the same
warning and `(declared by <file>)` as for waivers, and a wide entry that matches nothing here is not reported. This declares; it does not
prove: a dead item under a declared path is declared public too, so keep the globs narrow.

**A whole crate at once (`public_api_crates`).** A workspace of FFI-binding or published-library crates has hundreds of public items, and a path glob cannot name one crate from the workspace root (a member's finding paths are relative to the member, so `src/**` would match every member). Declare the package instead, in the **workspace root's** `.coderipper.toml`:

```toml
[reachability]
public_api_crates = [
  { crate = "baracuda-cuda-sys", reason = "FFI bindings: every pub item is API" },
]
```

`crate` is the cargo **package** name and `reason` is required. Every `reachability` finding of that package in a `--workspace` run is printed as `[declared-public/<severity>]` with the reason, like a `public_api` entry, and is not hidden. The declaration is read by the run of **each member**, so `--workspace` and a run by hand of one member (`--project crates/x`) give the same answer: a local run agrees with CI. A name that is no package of the workspace is reported as an `allowlist` finding naming the crate, from the `--workspace` run (the only run that knows every package); an entry for a package that has nothing to report this run is stale like any `public_api` entry. This declares; it does not prove: an item that is `pub` but not reachable through a `pub` module path is declared public too.

### Where a waiver, a `tracks` entry and `--deny` are read from

A `.coderipper.toml` is read **from the directory of the package being checked**, and from nowhere else. In a repository with a crate
at its root that is the root file. When the root has no crate and CodeRipper finds crates below it (0.6.0), **each nested crate reads
its own `.coderipper.toml`; the root file is not applied to it.** That errs toward **more findings reported**: a waiver
written at the root for a finding inside `backend/src/` does not waive it (put the entry in `backend/.coderipper.toml`, with `file`
relative to `backend/`); it does not make the entry stale either, because the root run never asked for that rule. Entries at the root
still apply to the rules the root run itself makes: the repository-wide and language-neutral rules (`ci-protection-presence`, and under
`--profile extended` `SUP-008`, `WSP-001` and the others). A `[[tracks]]` entry is read where `version-consistency` runs, the nested
workspace's root. `--deny` is a command-line option, not a file setting: it is one level for the whole run, nested crates included.
**One setting errs the other way:** `[buf] baseline` is read from the repository root only (API-006 judges the whole repository), so a
`[buf]` table in a nested crate's file is ignored and the default baseline is used, which can report fewer breaking changes than the
baseline you wrote. Put it in the root `.coderipper.toml`.

A waived finding does not fail the run, but it is **not hidden**: it is printed as `[waived/<severity>] ... -- reason: <reason>`
(a `coderipper-waived` line in JSON), so a reader of the output sees what was waived and why. With neither `symbol` nor
`lines` an entry waives every finding of that check in that file; a finding that names no line is never waived by an entry
that has `lines`. A `lines` value that is not a line or an inclusive range (`"0"`, `"20-10"`, `"abc"`) is an error, never a wider
waiver, and so is an unknown key (`line = 5`).

`symbol` is the bare name, not `Type::name`: two same-named methods in one file share an entry, so
one entry can hide a second finding with the same name (and that entry will not go stale while either
still matches). `file` is compared by path component, so `./src/api.rs` and `src\api.rs` also match; case
is not folded.

An entry that suppressed nothing in a run is reported as an `Info` finding (check id `allowlist`), so a
suppression whose cause has been fixed does not linger. An entry naming a check that does not exist is
reported the same way. An entry for a check that failed or did not run in that run is not judged. Both are findings, so
under the default they fail the run: fix or delete the entry.

## Configuration layers: a user or system file, the repository's file, the environment, flags

One file name, `.coderipper.toml`, is read from several places and combined, **weakest first**:

| # | Layer | May set |
|---|---|---|
| 1 | built-in defaults | - |
| 2 | `/etc/coderipper/.coderipper.toml`, or dotless `/etc/coderipper/coderipper.toml` (Windows: `%ProgramData%\coderipper\`) | everything |
| 3 | `/etc/.coderipper.toml` (Windows: `%ProgramData%\`) | everything |
| 4 | `~/.coderipper.toml` (`HOME`, else `USERPROFILE`) | everything |
| 5 | the repository's own `.coderipper.toml` (each member's, under `--workspace`) | **only** `[[allow]]`, `[[tracks]]`, `[buf]` and `profile` |
| 6 | environment variables (`CODERIPPER_CACHE`, `CODERIPPER_CACHE_DIR`, `CODERIPPER_TOOLS_DIR`, ...) | what they always set |
| 7 | command-line flags | per-invocation values and defaults |

**Every file found is read; the first one found does not hide the rest.** A later layer overrides an earlier one, so a user's file
overrides `/etc`, the repository's `profile` overrides both, and a flag you type overrides everything. The wide files (2 to 4) are
read by the command-line program only: a library caller that never asks sees none.

```toml
# ~/.coderipper.toml: defaults for every command-line option that is a default, and what the environment variables set
deny = "medium"          # --deny: none | info | low | medium | high | critical
profile = "extended"     # --profile: extended (the default) | classic | portfolio
message-format = "human" # --message-format: human | json
redact = false           # --redact
install-tools = false    # --install-tools

[cache]                  # CODERIPPER_CACHE=off, _DIR, _MAX_GB, _WAIT_SECS
enabled = true
dir = "D:/coderipper-cache"
max_gb = 20
wait_secs = 5

[tools]                  # CODERIPPER_TOOLS_DIR
dir = "D:/coderipper-tools"

[limits]                 # ceilings: see below
max_runtime_minutes = 30
```

**The repository's file is an allowlist, and fails closed.** The repository is untrusted input (in a hosted service it is someone
else's code), so a key that is not on the allowlist is not honoured from it: a *known* key outside the list (`deny`, `redact`,
`install-tools`, anything under `[cache]`, `[tools]` or `[limits]`) is **skipped with a warning that names the key and the file**
and the run goes on; an *unknown* key (a typo) is an error, as it always was. That keeps out a repository that points `tools.dir`
at its own directory so CodeRipper runs its binary, grants itself `install-tools`, turns off `redact`, sets `deny = "none"` or
raises a limit. `profile` is on the list because it is a statement about the project's own rule set; `deny` is not, because a
project's CI sets it with a flag.

**Limits are ceilings, combined by minimum.** For every `[limits]` key (`max_runtime_minutes`, `max_memory_mb`, `max_disk_mb`,
`max_processes`) the effective value is the smallest any layer gave (the files, and `CODERIPPER_MAX_RUNTIME_MINUTES` and so on), so
a later layer can lower a ceiling and never raise it. CodeRipper **reports** the ceilings in force on stderr and does not itself stop
at them (yet): whatever runs CodeRipper must enforce them from outside, because a limit read from a file inside the job's own
container is a request, not a boundary.

**`[[allow]]` and `[[tracks]]` in a wide file** are accepted, and they are the wrong place: a waiver is a statement about one
repository and belongs in that repository's file, where it is reviewed with the code. Each run says so once on stderr, naming the file
and the number of entries, and applies them anyway. A finding waived by a wide file is printed with `(waived by <file>)` (JSON:
`waived_by`), so a reader can tell a waiver nobody in the repository reviewed. A wide entry that matches nothing in this repository is
**not** reported as stale (it may be written for another repository); an entry in the repository's own file still is.

**Reproducibility.** The same repository must give the same answer on CI and on a laptop, or the report must say why not. Each run
that read a wide file names it on stderr (`coderipper: configuration: <files>`). `--config <path>` (or `CODERIPPER_CONFIG`) reads
exactly that file in place of layers 2 to 4, and it must exist; `--no-config` (or `CODERIPPER_NO_CONFIG=1`) reads none of them, and
is how to check that a result does not depend on the machine. Neither affects the repository's own file. A wide file that cannot be
read or parsed stops the run with exit 2, naming the file.

## Why

Found by hand, once, during an unrelated design review: a fully correct, well-tested module in another
project had zero callers anywhere in that project's serving path. The logic was right; nothing called it.
That's the defect class this tool exists to catch systematically, not by luck of a review going looking in
the right place.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE), at your
option.
