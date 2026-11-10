<p align="center"><img src="docs/assets/coderipper-logo-claw.webp" alt="CodeRipper: claw slashes over glowing circuitry" width="240"></p>

# CodeRipper

Audits a project, in any language it holds, for the integration defects that compilers and linters do not report: public
functions nothing calls (`rustc`'s `dead_code` lint deliberately skips `pub` items), return values every caller discards,
parameters nothing reads, packages in one project that disagree on their version, and a repository whose default
branch enforces no CI. Each is a pluggable check under one host that reports in one `Finding` shape.

CodeRipper started with Rust and is built to audit interwoven projects in as many languages as they contain. Today the
deepest checks (`reachability`, `unused-return-values`, `unused-parameters`, `version-consistency`) read Rust, and the
language-neutral rules (`--profile extended`) run on any repository. A rule that does not cover a project's language is
reported as *not run here*, never as a failure. To contribute, see
[CONTRIBUTING.md](CONTRIBUTING.md).

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

The Rust checks (`reachability`, `unused-return-values`, `unused-parameters`, `version-consistency`) judge a cargo package. `reachability` grades its finding **Info** (from 0.32.0; it was Medium for every finding) when the item is documented public library API reachable through `pub` modules, or has no live caller in the library and is named in the crate's own tests (`#[cfg(test)]` items and test files); every other finding stays Medium, and matching is by bare name, so a dead item that shares its name with something a test calls is Info too. Where
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

**`--top N` (0.18.2): a brief report.** `coderipper sweep --top 10` prints the ten most severe findings (ties: the most confident first) and ends with `coderipper: N more finding(s) not shown (2 high, 1 medium)`. JSON keeps ten `coderipper-finding` lines and adds `findings_omitted` to the `coderipper-summary` line, whose `findings` is still the full count. It changes no exit code.

**`--summary` (0.9.8): one line per rule.** `coderipper sweep --summary` ends the human output with a totals line and one line per rule (findings, `[[allow]]` waivers, `public_api` declarations, severities); with `--message-format json` it prints one `coderipper-rule-summary` line just before the `coderipper-summary` line. The field names are stable: `reason`, `project`, `findings`, `waived`, `declared_public`, `opt_outs` (waived plus declared public: the project's visible opt-outs), `errors`, and `rules`, a list sorted by rule id whose items have `rule`, `findings`, `waived`, `declared_public` and `severities` (lower-case severity to count). A rule that ran clean has no row, and `--summary` changes no exit code.

## As a library

```toml
[dependencies]
coderipper = { version = "0.43", default-features = false }   # no clap, no binaries
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

A project whose crates each version by their own interface (CireSnave's ruling, board 295: each interface crate keeps its own
version, and they meet at one number at major releases) lists those crates with `[[interface]]`; the reason is required. An
interface crate is left out of the one-version comparison, so interface crates may differ from each other and from the project
between major releases; the root package and the tools follow the project number as before. When the project is at a major release
(`X.0.0`, or `0.Y.0` before 1.0) every interface crate must be at that number, and one that is not is a finding:

```toml
[[interface]]
package = "fireline-cuda"
reason = "versions by the CUDA interface it wraps"
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
| `ARC-001` | Rust workspaces with an `[architecture]` table in `.coderipper.toml` (default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no declaration, or one with no layers, gets "not run" in the coverage, never clean**): the dependencies between workspace crates (`cargo metadata --no-deps`, normal and build dependencies by `path`) against the declared layers (a crate may depend on its own layer or any layer below it) and `[[architecture.allow]]` edges (each with a reason). A waiver (`[[architecture.waiver]]`: edge, reason, owner, end date) is printed as a note on every run; an expired one no longer covers its edge. **Low:** an edge that goes up a layer or is not allowed. **Info:** a declared component that names no crate, a crate the declaration does not place, a waiver for an edge that is not a violation. Module groups inside a crate (a component whose `paths` name files, directories or `*`/`**` patterns under `src/`) are judged from the crate's module graph, read from the source with `syn`: a path starting `crate`, `self` or `super` (or naming a child module), and each `use`, is an edge from the file that holds it to the file it names; a path inside a macro call's tokens, an alias or a glob re-export is NOT followed, so the graph is a lower bound; a group that matches no file is **Info** stale. Dev-dependencies are not judged. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-002` | Rust workspaces with an `[architecture]` table that declares a component or a layer (default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no declaration gets "not run" in the coverage, never clean**): the component graph that `ARC-001` reads (the edges between workspace crates, `cargo metadata --no-deps`, and the edges between module groups of a crate, read from the source), each edge mapped to the components that hold its ends, and the strongly connected sets of two or more components (Tarjan). **Low**, one finding per set: the number of components, the edges between them, and the edges whose removal dissolves it (every single edge that does when one is enough, otherwise a depth-first set in name order, not necessarily the smallest). **High** confidence when only crate edges are in the set, **Medium** when a module group is. Cargo refuses a cycle between crates, so a cycle is between components of more than one crate or between module groups. Not reported: cycles inside one component, a cycle through a macro call's tokens or an alias (the module graph is a lower bound); an `allow` edge or waiver of `ARC-001` does not excuse a cycle. No false-positive rate is claimed. Allowlist the component names, sorted and joined by commas (`a,b,c`), with the reason. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-003` | Rust workspaces with an `[architecture]` table that has an `[architecture.volatile]` table and a component tagged `volatility = "stable"` (role `policy` or none) (default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no declaration, no volatile table or no stable component gets "not run" in the coverage, never clean**): in the tracked `.rs` files under `src/` of each crate of such a component (test items skipped), the uses of the declared `names` (any identifier equal to one), the uses of the declared `namespaces` (`sqlx::` is the tokens `sqlx` `::`), and, with `feature_cfg_in_policy`, the `cfg`/`cfg_attr` attributes that test a Cargo feature. One finding per component with the counts, the most affected files and the features; the components are ranked in a note. **Low:** a name or namespace is used. **Info:** only feature attributes. Medium confidence, syntactic: an identifier equal to a declared name is counted whether or not it is that type, a type alias or a macro's output is NOT SEEN, `cfg!(feature = ..)` calls and `build.rs`, `tests/`, `benches/` are NOT READ, module groups inside a crate are noted and not judged. No false-positive rate is claimed. Allowlist a component (subject = its name) with the reason. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-004` | Rust workspaces with an `[architecture]` table that has a policy component (`role = "policy"`, or no role and `volatility = "stable"`) and something declared as infrastructure (`[architecture.volatile]` names or namespaces, `[arc004] patterns`, or a component that is volatile, an adapter or infrastructure) (default on in `extended` and `portfolio`, not `classic`; advisory **Medium** at medium confidence, never gating; **a repository that declares none of this gets "not run", never clean**): per public signature of a policy component's library (a `pub` function, a `pub` method of a public type, a public trait's items, `pub` fields, enum variant fields, a public alias, constant or static), the types it names that resolve, through the module's `use` paths, to a declared infrastructure namespace or name or to a crate of an infrastructure component. A public alias of such a type is looked through. One finding per signature, subject `<component>:<item>`, so a deliberate exception is an allowlist entry with a reason. Syntactic: glob imports, macro-generated items and aliases in other crates are not seen; `ARC-003` counts the tokens per component, this reports where a signature leaks |
| `ARC-005` | Rust workspaces (default on in `extended` and `portfolio`, not `classic`; advisory, never gating): the component graph that `ARC-002` reads (the edges between workspace crates, and between the declared module groups of a crate; with no `[architecture]` table, each workspace member is a component), the instability `I = Ce / (Ca + Ce)` of each component (`Ce` the components it depends on, `Ca` those that depend on it), and every edge `A -> B` where `I(B) - I(A)` reaches the margin, ranked by it. **Info**, low confidence, one finding per edge with both ratios and the neighbours counted. The margin is 0.3 unless the repository's own `.coderipper.toml` sets `[arc005] min_margin` in `(0, 1]` (anything else is an error). Not reported: an edge to a component declared `volatility = "volatile"`, an edge inside a cycle (that is `ARC-002`'s), an edge to a crate outside the workspace; a repository of one crate with no declared module groups has no edge, and the note says so. Instability counts neighbours, not how often a component changes, so a frozen dependency can be flagged; Martin's abstractness is not used. Measured 2026-11-04 on committed trees (coderipper 3f5eb81, auth-framework 881c696d and releaseant d773059: one crate, 0 edges, so nothing to judge; Unpopped dcc3174: 4 components, 5 edges; PersistAnt 32ccc43: 9, 18; configurant dd7a331: 6, 7; httpant 466d8ad: 6, 9; synapse 40ec84f: 9, 17; Fuel 627c2bb7c: 45, 166): 0 findings at the default margin 0.3 in every repository, and 0 at a margin of 0.05 in the five small multi-crate ones (Fuel and the single-crate repositories were not re-run at 0.05). The same binary reports 1 finding on the seeded fixture `conformance/ARC-005/defective`. A false-positive rate is not claimed, because there was nothing to read: the crate-level reading is quiet on a layered workspace, and the module-level reading needs declared module groups. Text only: it makes no change. Allowlist the subject `a->b` with the reason. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-006` | Rust workspaces with an `[architecture]` table that has `[architecture.api_budget]` entries (a component name and the most public items it may offer) (default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no declaration or no budget gets "not run" in the coverage, never clean**): in the library (`src/lib.rs`, tracked `.rs` files under `src/`, test items skipped) of each crate of a budgeted component, the public **items** (a `pub` function, struct, enum, union, trait, type alias, constant, static, module, `#[macro_export]` macro, or `pub use` leaf, reachable only through `pub` modules) and the **members** of the public types (pub fields, enum variants, trait items, inherent `pub` associated items, wherever written), both stated, their sum compared with the budget. **Low:** over budget. **Info:** a `pub use` rooted in a library the component does not own (one finding per component, subject `<component>:third-party`); a budget entry that names no declared component. Medium confidence, syntactic: macro-generated items and `#[path]` modules are NOT SEEN, a `cfg`-gated item counts as enabled, same-named types in different modules share members, a glob re-export counts once, and a third-party type in a public signature is NOT SEEN (API-008). Module groups inside a crate are noted and not judged. No false-positive rate is claimed. Allowlist a component (subject = its name) with the reason. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-007` | Rust workspaces with an `[architecture]` table in which a component declares `owns_data` (the tables, collections or topics only it may name) (default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no declaration or no `owns_data` gets "not run" in the coverage, never clean**): in every other component's crates, the string literals of the `.rs` files under `src/` (test items skipped; literals inside macro calls such as `sqlx::query!` and `format!` are read) that are exactly the name, or are shaped like SQL (a word among select, insert, update, delete, join, into, table, alter, truncate, drop, create) and hold the name as a whole word, case-insensitively, and the `.sql` files under the crate directory (lines outside `--` comments naming it). **Low**, low confidence: one finding per (name, other component) with the count and the files; **Info:** a name declared owned by two components. NOT SEEN: a name built at run time or read from configuration, an ORM that maps a type to a table, crates that are not Rust; a log label or an SQL-shaped message that holds the name is counted. Module groups inside a crate are noted and not judged. No false-positive rate is claimed. Allowlist `<component>:<name>` with the reason. Format and background: `docs/rules/arc.md` in the repository |
| `ARC-008` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory **Info** at low confidence, never gating; the shapes only, the check does not judge that a layer "earns nothing"): **a trait with one implementation and no test double**: a trait of the package with at least one method, implemented (`impl Trait for X`) exactly once in the non-test code of the package, never in test code (`#[cfg(test)]` items, test files by name), the implementing type not named `Mock`, `Fake`, `Stub`, `Dummy`, `Spy`, `InMemory`, `Noop`, `Null` or `Test`; not exempt: a `pub` trait of a library (consumers may implement it, same public-surface reading as `ARC-006`), a supertrait, a blanket-implemented trait, a marker trait with no method. **A chain of forwarding types**: a struct with one field (through `Arc`, `Rc`, `Box` or a reference) whose every method taking `self`, at least two, is one call to that field with the same arguments, when the field's type is a forwarding struct of the package too, reported once at the first type. Not done: chains of near-identical data mappers and long call paths before any I/O (they need data flow). Syntactic: types and traits are matched by name; an implementation made by a macro is NOT SEEN. Measured on seven of our repositories on 2026-11-03: **0 findings in all seven** with the public exemption; with it switched off the same read finds 9 traits, all `pub` traits of libraries (coderipper's `Cargo` and `Github`, five in auth-framework, two in synapse), which are ports or extension points, which is why public traits are exempt; no chain of forwarding types exists in any of the seven. No false-positive rate is claimed beyond that. Text only: no change is offered. Allowlist the trait or first type name with the reason |
| `ARC-009` | Rust, from the source stored in git (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating; **a repository with no release tag `v<version>` reachable from `HEAD` and no `[arc009] base` gets "not run" in the coverage, never clean**): per workspace crate, two numbers at `HEAD` and at the base (the newest `v<version>` tag that is not `HEAD` itself, or `[arc009] base`: a tag, branch or commit), read from the object store with no checkout: the **module cycles** (the count `MOD-002` reports) and the **public surface** (public items plus members of the public types, the sum `ARC-006` budgets). **Low:** more module cycles than at the base. **Info:** the surface grew by more than 25% **and** by at least 10 items (`[arc009] surface_growth_percent`, `surface_min_items`). A crate absent at the base is new and not compared. Low confidence: a release that adds a feature grows its surface on purpose; the finding gives both numbers and the base and asks. Not done: instability margins and violations of declared layers over time (they need the crate graph and the declaration at the base), waiver counts and ages. Measured on 2026-11-04 on eight of our repositories at their newest release tag: **1 finding** (httpant-server: a cycle between `actix_adapter` and `actix_tls` that v0.5.22 did not have, confirmed with `MOD-002`), 0 in the other seven; 7 crates of httpant and synapse were new since their tag and not compared. No false-positive rate is claimed beyond that. Allowlist `<crate>:cycles` or `<crate>:surface` with the reason |
| `ARC-011` | Rust, from the source (`syn`) and the committed history (`git log`, no network, no checkout; default on in `extended` and `portfolio`, not `classic`; advisory, never gating): per workspace crate, a pair of top-level modules that **each name the other** (the edges `MOD-002` reads) **and** were changed together: at least 3 commits touched a file of both (`[arc011] min_commits`) and they are at least 60% of the commits that touched either (`min_share_percent`), over the newest 500 non-merge commits (`window`; a commit touching more than 25 files is set aside, `max_files`). **Info**, low confidence: two modules that cannot be understood apart and are never edited apart are one module split in two. Only direct mutual references between two top-level modules of one crate are read, not a service graph across repositories; renames are not followed and squash-merge history makes commits coarse. Allowlist the subject `<crate>:<a>,<b>` with a reason |
| `ARC-012` | Rust workspaces with an `[architecture]` table that names composition roots (`[architecture.composition] roots`, or a component with `role = "composition"`), an adapter or infrastructure component and a policy component (default on in `extended` and `portfolio`, not `classic`; advisory **Medium** at medium confidence, never gating; **a repository that declares none of this gets "not run", never clean**): a policy component's code that builds a concrete adapter itself, a struct literal or constructor-looking call of a struct from an adapter component that implements a port, resolved through `use` paths; roots, `src/main.rs`, `src/bin/**` and test code are exempt. One finding per site, subject `<component>:<crate>::<Type>`, so a deliberate exception is an allowlist entry with a reason. Syntactic: glob imports, aliases, macros and factory functions elsewhere are not seen |
| `CON-006` | Rust, from the source (`syn`): a lock is named by the path that reaches it (`self.field`, `self.a.b`, `self.shards[i]`, a `static`), and a guard held while another lock is acquired is an edge (a `let`-bound guard to the end of its block or `drop`, a temporary to the end of its statement; a plain `if` condition ends before the body, an `if let` scrutinee and a `match` scrutinee live through it). A **cycle** among the edges of the repository (`a -> b` in one function, `b -> a` in another) and a lock taken while it is already held are reported at **High** severity and **Low** confidence: a possible deadlock, read from direct nesting only. Not seen: a lock reached through a local variable or a parameter, an edge made by a call, a guard moved into a function or closure, macros, tests, benches and examples. A pass is evidence about the nesting `syn` can see, no more. |
| `ERR-001` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check ERR-001`, in no profile): a failure dropped with no stated reason, outside tests and `impl Drop`: `let _ = call();`, `_ = call();`, `call().ok();` as a statement, an `Err(_) => {}` arm, and `if let Ok(..) = call` with no `else` when the call is a file-system, file, socket or `Command` call or a read, write or flush method. **Medium** severity at medium confidence, one finding per site. A comment on the same line, up to the end of the statement, or alone on the line above is the stated reason the rule asks for and silences the site (the opposite of `lint-suppression`). A bare `call();` that discards a `Result` is `unused-return-values`. Not seen: other languages, whether the dropped value is a `Result` (`syn` has no types), a drop through a macro, a function that swallows internally. Measured on eight repositories: 191 sites, 22 in CodeRipper itself, which is why it is not default-on. |
| `EXT-003` | Rust, from the source (`syn`; `portfolio` and `extended`; **Medium**): in an `impl Trait for Type` (not `Drop`), a method whose body calls `todo!` or `unimplemented!`, or is at most three statements with no branch or loop and holds a string saying `not supported` or `not implemented` (the methods of `Display` and `Debug` impls are not read for the phrase). Trait methods with default bodies are not looked at. Cannot see `panic!` stubs, silent empty overrides or macro-generated impls |
| `MOD-001` | Rust libraries, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating): per package, the public declarations (function, struct, enum, union, trait, type alias, constant, static, module and `#[macro_export]` macro, reachable through `pub` modules, test items skipped) and the public inherent functions of public types that carry no documentation: no `///`, no `#[doc = ..]` or `#[doc(..)]` other than `hidden`, and, for a file module, no `//!` inside its own file. A `#[doc(hidden)]` item is in neither list; a plain `//` comment is not documentation. One finding per package with both counts the design asks for (the public **items** and the **members** of the public types), the gaps and the first places. **Info** for any gap; **Low** from 50% of at least 20 documentable items (a chosen figure, far above the shares read on our code, not a percentile). The size of the surface is stated, not judged; a ceiling is `ARC-006`'s. Syntactic: macro-generated items and doc comments are NOT SEEN, trait items and enum variants are not asked for documentation, `#[path]` modules are not read. Measured on 2026-11-02 (committed trees): 0 of 233 undocumented in coderipper, 0 of 97 in PersistAnt, 1 of 200 in releaseant, 26 of 226 (11%) in configurant, 380 of 4,762 (8%) in auth-framework, 237 of 1,152 (21%) in synapse; 16 of the gaps in the last two were read in the source and every one was a real gap. Not done: the reachability precision of the design note's E8 and the delegated `unreachable_pub` and `missing_docs` lints. Allowlist the package name with the reason |
| `MOD-002` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating): in the module graph of a package (the tracked `.rs` files under `src/` reached from `lib.rs`/`main.rs` through `mod`; test items skipped), the strongly connected sets (Tarjan) of two or more modules, an edge being a path written in an item (or a `use`) in one module that names an item inside another. A child naming its parent through `super` or `self` is not an edge; a module reaching back to one that contains it counts only through a `crate::` path; the crate root takes no part (`crate::Error` in `lib.rs` is the façade); the `use` that builds a `pub use` is not an edge (the design note's question 2, answered as it proposed). **Low** when the set spans two or more top-level modules, **Info** when it is inside one; medium confidence. One finding per set with the number of modules, the edges (file and line each) and the edges whose removal dissolves it: every single edge that does when one is enough, otherwise a depth-first set in name order, not necessarily the smallest. NOT SEEN: a path inside a macro call's tokens, an alias, a glob re-export; `#[path]` modules are not read. No levels, tree, metric or ranking of the edges is computed (patent watch). Measured on seven of our repositories on 2026-11-02: 0 findings in four, 7 sets in three (coderipper 2, auth-framework 3, configurant 2); 16 edges of those sets were read in the source and every one is real. Allowlist the subject (the module paths sorted, comma-joined) with the file of the first module and the reason |
| `MOD-003` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory Low at medium confidence, never gating): in the module graph of a package (the tracked `.rs` files under `src/` reached from `lib.rs`/`main.rs` through `mod`; test items skipped), a path written in an item, or a `use`, that resolves to an item inside a module that is at least one level below a sibling's root (`crate::a::b::X` written outside `a`) **when a module on the way (`a` or `a::b`) re-exports that item** with `pub use`, `pub(crate) use` or `pub(super) use` (by name, renamed, or by a glob of the module that holds it): the caller bypassed the entry point the owner offers. One finding per file with the count, the first line and the shorter path offered (`crate::a::X`). A path inside the caller's own module subtree, the `use` that builds the re-export, and a deep path with no re-export in the way are not reported (this is the narrow reading of the design note's depth question). Syntactic: a re-export made through a macro or an alias is NOT SEEN, a path inside a macro call's tokens is NOT READ, a glob re-export is taken to offer every item of the module it globs. Measured on seven of our repositories on 2026-11-02: 0 findings in six, 33 files in one (auth-framework); every module pair checked in that repository's source was a real bypass of a re-export (see CHANGELOG); no false-positive rate is claimed beyond that reading. Allowlist a file (subject = its path) with the reason |
| `MOD-004` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory Info at low confidence, never gating): a struct whose methods split into usage clusters. Read at `HEAD` from the tracked `.rs` files under `src/`; test items and trait-impl methods are not read. A method joins another's cluster when they use a common `self.field`, directly or through the sibling methods they call (arguments of macros such as `format!` are read as tokens). Accessors, setters, derived values (at most two statements, two fields and no sibling call) and builder steps (`self` by value) belong to no cluster. A type is reported when two or more clusters each hold two or more methods, among four or more in all; the finding names each cluster's methods and fields. The figures were chosen by reading the splits found in our own codebases, not taken from a percentile. Syntactic: a field reached by destructuring or passed through a helper is NOT SEEN, so clusters can look more separate than they are; the type may equally be independent helpers. Allowlist a type (subject = its name) with the reason. |
| `MOD-005` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating): a file or a type far above the size and coupling of our own codebases. Read at `HEAD` from the tracked `.rs` files under `src/`; test items (`#[cfg(test)]`, `#[test]`) and files named `tests.rs`, `test.rs`, `*_test.rs`, `*_tests.rs` are not measured. **Info:** a file of 1,035 or more code lines (not blank, not a `//` line), or one that names 14 or more other modules (a file many others name is NOT reported for that alone: a small stable module everything depends on is the point of depending toward stability), or a type with 30 or more fields plus inherent methods and at least one method. **Low:** a file of 2,234 or more code lines that also names 7 or more other modules or is named by 6 or more other files. The thresholds are percentiles (p95 and p99) of the pooled corpus of nine of our own codebases at `origin/main` on 2026-11-01, not of the project alone, and each finding states the value, the threshold and that source. Size is a cue to inspect, never a verdict. Syntactic: macro-generated items, glob-import targets, `#[path]` modules are NOT SEEN, so coupling is a lower bound; growth over time is NOT MEASURED (needs history). Allowlist a file (subject = its path) or a type (subject = its name) with the reason. |
| `MOD-006` | Committed history (`git log`, no network, no checkout; default on in `extended` and `portfolio`, not `classic`; advisory **Info** at low confidence, never gating): per source file at `HEAD` (a tracked `.rs`, `.py`, `.ts`, `.tsx`, `.js`, `.jsx`, `.go`, `.java`, `.kt`, `.c`, `.cc`, `.cpp`, `.h`, `.hpp` or `.cs` file; not a registry (`lib.rs`, `main.rs`, `mod.rs`, `__init__.py`, `index.ts`, `index.js`), build script, test, bench or example), the newest 500 non-merge commits that touched it, two of them joined when they touched a source file in common besides it; the file is reported when **two or more groups of at least 3 commits** result, with the groups, their commits and files. Set aside, and said in a note: a commit touching more than 25 files (a rename, a format run, a release), a file that over a fifth of the commits touch (`Cargo.lock`, the changelog), a document or workflow edited with the file, a registry. `[mod006] window`, `min_commits` and `max_files` in the repository's own `.coderipper.toml` change the three numbers. Measured on 2026-11-04 on seven of our repositories and coderipper itself (committed history, the newest 500 commits of each): **1 finding at the defaults** (PersistAnt's `vocabulary.rs`, edited with the key-value crate's files in 14 commits and with the blob crate's in 3: read, a shared vocabulary that two backends grew by turns), 0 in the other seven; with `min_commits = 2` 8 (PersistAnt 3, Unpopped 2, synapse 2, coderipper 1), 8 read, 6 of them plausible reasons to change apart (a macro implementation edited with its checker and with its sqlx bridge, `cli.rs` edited with every option and with the tool installer) and 2 weak (a group made only of tests or benches); with `min_commits = 1` 75, mostly pairs of single commits, which is why the default is 3. Squash-merge history makes commits coarse, so unrelated changes in one pull request join; renames are not followed; a shallow clone is noted. A hub (a table, a registry of another name) is edited for many reasons by design. Text only: no fault count, no ranking, no split is offered. Allowlist the file path with the reason |
| `MOD-008` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory **Info** at low confidence, never gating): a free function or an inherent method whose whole body is one call that forwards **all** its parameters, in order, unchanged: `g(a, b)`, `self.g(a)` (an alias for another method of the type) or `g(&self.x, a)`, the call possibly wrapped in `Ok(..)`, `Some(..)`, `?`, `.await` or `.into()` or written `return ..;`. At least one parameter besides `self` must be forwarded. **Not wrappers:** a call on a field (`self.inner.go(v)`, `self.rules.push(r)`: they hide the field behind an interface, information hiding or a facade over a manager), a constructor (`Self(x)`, `Wrapper(x)`, `Self::Variant(x)`: a callee whose last segment starts with a capital), a function with nothing to forward, a trait impl method (the signature is dictated), a `pub` function written in `lib.rs` or `main.rs` (a facade), test items. One finding per wrapper naming the pair (wrapper, callee) and the reasons a layer may still be worth keeping (a name that fixes a meaning, a narrower type, a place for an invariant, a stable API over an unstable callee). Syntactic and per file: the callee is named, not resolved; a macro body is NOT READ. Measured on seven of our repositories on 2026-11-03 after the shape was narrowed: 0 in four (coderipper after removing its two own, PersistAnt, configurant, httpant), 35 in three (auth-framework 30, synapse 4, releaseant 1); the first definition flagged 228, almost all `self.field.method(..)` delegation and `Self(x)` constructors, which is why those are excluded; the 52 left after the field calls were excluded were read line by line. Text only: no change, patch or option list is offered. Allowlist the subject (`name` or `Type::name`) with the reason |
| `MOD-009` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating): the package's own structs are read; a field whose type is a different struct of the package, and an inherent method that takes only `self` and returns a different struct of the package, are hops. A method-call or field-access chain whose consecutive links each follow a hop from the type the previous link reached is as deep as its run of hops; a chain of at least `[mod009] min_hops` (default 4, at least 2, the repository's own `.coderipper.toml`) is one finding at its first hop. Builders (they return `Self`) and iterator pipelines (they return standard-library types) are not hops; a method that takes an argument, an `Option` or a `Result` ends the run; test items are skipped. Matching is by name, so a configuration tree or a syntax tree that exists to be traversed reads as a chain. The note prints how many chains there are at each depth. Measured 2026-11-04 on committed trees with the depth histogram the check prints (every package of each repository): coderipper 3f5eb81: 1 chain of 3 hops; auth-framework 881c696d: 193 of 2 hops and 37 of 3; synapse 40ec84f: 56 of 2 and 5 of 3; configurant dd7a331: 15 of 2; PersistAnt 32ccc43: 2 of 2; releaseant d773059: 2 of 2; Unpopped dcc3174 and httpant 466d8ad: 0 of 2 or more. The deepest chain anywhere is 3 hops, so the default of 4 reports 0 findings on every repository, and a seeded chain of 4 is reported (`conformance/MOD-009/defective`). All 43 chains of 3 hops were read: 37 are `parent.config.<section>` or `builder.config.<section>` in auth-framework's sub-builders (a builder that writes one section of a configuration it holds), 5 are `config.smtp.password` and `config.imap.password` in synapse (a configuration tree read on purpose) and 1 is `concrete.location.start_point` in coderipper's parser of a tool's JSON report; none is a traversal that one would ask to be hidden behind a method, which is why the default is 4 and `[mod009] min_hops = 3` is the way to see them. Text only: it names the chain and makes no change. Allowlist the chain text (`customer().address().city()`) with the reason. Format and background: `docs/rules/mod.md` in the repository |
| `MOD-010` | Rust, from the source (`syn`; default on in `extended` and `portfolio`, not `classic`; advisory, never gating; the declared half has nothing to judge without `invariant_types`): **declared half** (a type named in the `invariant_types` of a component, found by name in the `src/` of that component's crates, test items skipped): **Low** when it has a `pub` field (a tuple field by its index); **Info** when it derives `Default` or `Deserialize` (without `#[serde(try_from = ..)]` or `from`) while it has a validating-looking constructor, and when the name is a struct in none of the component's crates (with a positive control). **Inferred half, opt-in** with `[mod010] inferred = true` in the repository's own `.coderipper.toml` (a system or user file does not turn it on): **Info** at low confidence for a `pub` struct, declared or not, with a validating-looking constructor (an inherent `new`, `try_new`, `parse` or `from_*` that takes an argument, returns `Result<Self, _>` or `Option<Self>` and names `Err`, `None`, `bail!` or `ensure!` in its body; `?` alone does not count) and a `pub` field. NOT SEEN: a derive made by a macro, a manual `impl Default` or `Deserialize`, a builder, a type matched only by name across modules. Measured on seven of our repositories on 2026-11-02, the inferred half only (no repository of ours declares `invariant_types` yet, so the declared half was not measured): 0 findings in four, 14 in three; read constructor by constructor, 6 guard an invariant, 4 are wire-format parsers whose invariant is debatable and 4 are plain data records, which is why it is opt-in. Text only: it names the fields and makes no change. Allowlist the type name with the reason. Format and background: `docs/rules/mod.md` and `docs/rules/arc.md` in the repository |
| `OPS-009` | Prometheus rule files (`*.rules.yml`, `alerts*.yml`, `alert_rules*.yml` and the `.yaml` forms; advisory **Info**), parsed as YAML: an alert under `groups[].rules[]` with no non-empty `annotations.runbook_url`. One finding per alert, at its `alert:` line. Not applicable where no rule file holds an alert. Alert rules in Helm templates or `PrometheusRule` resources, and a runbook linked only from `description` text, are not seen |
| `OPS-011` | Rust binaries (default on in `extended` and `portfolio`, not `classic`; builds the package's binaries from `HEAD` and runs them, so it executes project code): each binary built from the committed tree is run through `--version`, `-V`, `--help`, `-h` and an unknown flag, each in a fresh temporary directory with an empty environment (only `PATH` and the Windows system variables) and a 10-second timeout. It expects exit 0 and a version line containing the manifest version, usage on stdout, exit 2 with the message on stderr and stdout empty for the unknown flag, no ANSI escape sequence in any output, and nothing created or changed under the working directory or the directories `HOME`, `APPDATA`, `LOCALAPPDATA`, the `XDG_*` roots and the temp directory point at. A probe that times out is run again, and makes the finding Medium; any other failure is Low. Allowlist a binary that cannot answer (subject = its name) with the reason. Rust only: `package.json` `bin` entries and Python console scripts are NOT CHECKED. |
| `OPS-012` | Rust binaries, from the source (`syn`; `portfolio` and `extended`): a fixed editor (`nano`, `vim`, ...) in a file that never names `EDITOR`/`VISUAL`; a dotfile joined onto the home directory in a file that never names XDG, `APPDATA`, `config_dir` or a directories crate; `env::args` with two or more flags and no argument-parser dependency in `Cargo.toml`; `-v` printing the version or `-V` setting verbosity; and, only for a feature the binary offers, `--yes`/`--quiet`/`--verbose` without `-y`/`-q`/`-v`, stdin read with no `-` operand, a colour crate that ignores `NO_COLOR`, a never-ending loop or listener with no Ctrl-C handling. Low severity; Medium confidence (Low for the stdin and Ctrl-C cases). Not judged: other languages, a name in another file, `format!`-built paths, a run of the binary |
| `PRF-003` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check PRF-003`, in no profile; advisory **Medium** at low confidence): per function, one finding at the first site for four shapes inside a loop (a `for`, `while`, `loop`, or the closure of an iterator adaptor over an iterator): a linear search (`contains`, `iter().any`, `iter().find`, `iter().position`) of a `Vec`, `VecDeque`, slice or array declared outside the loop; `s = format!(.. s ..)` rebuilding a string declared outside it; a `query`, `query_as`, `query_row`, `prepare` or similar call with a `SELECT` literal; and a `clone`, `to_vec` or `to_owned` of a `Vec`, `String`, `HashMap` or similar collection declared outside it whose result is only read (`.iter()`, `.len()`, `.contains()`, `.get()` and the like) or iterated by a `for` inside another loop (a clone that is handed on is not read); the closures of `map_err`, `ok_or_else`, `unwrap_or_else` and `or_else` are not read. A type is known only from a typed parameter, a typed or constructed `let`, or a field of the same file. Not read: a collection whose type is not declared, a search inside a called function, per-row `INSERT`, `UPDATE` or `DELETE`, whether the loop is large, whether a clone is needed, other languages. A cue to read, never a verdict; the allowlist subject is `Owner::function` or the function name. |
| `PRF-005` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check PRF-005`, in no profile; advisory **Medium** at low confidence): one finding per growable collection (`Vec`, `VecDeque`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`, `BinaryHeap`, `LinkedList`, `IndexMap`, `IndexSet`, `DashMap`, `DashSet`, looked through `Mutex`, `RwLock`, `RefCell`, `Arc`, `Rc`, `LazyLock`, `OnceLock` and the like) that is a `static` or `thread_local!` static, or a named field whose name contains `cache` or `memo` (not `memory`) (or any field of a struct named so), at its declaration, when it is inserted into or pushed to (`insert`, `push`, `push_back`, `extend`, `append`, `entry`, ..) and nothing in the repository removes from it, evicts, clears or truncates it, reassigns it, `mem::take`s it or compares its `len()`. Names are matched across the repository by identifier, so a bound on another type's field of the same name hides a candidate. Not read: whether the collection takes external input, growth or a bound through a returned reference, a macro or an alias the rule does not follow, types that bound themselves (`lru::LruCache`), locals and fields not named like a cache, tests, other languages. A cue to read, never a verdict; the allowlist subject is the static's name or `Owner::field`. |
| `RDB-004` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check RDB-004`, in no profile; advisory **Low** at medium confidence): per function outside tests, one finding for each of three measures. **Parameters:** more than 7, `self` not counted (clippy's default; not read in the methods of a trait impl, whose signature the trait dictates). **Nesting:** control flow nested 5 or more deep (`if`, `match`, `loop`, `while`, `for`; an `else if` shares the level of its `if`, a closure adds none, an inner function is measured on its own; read in trait impls too). **Flag argument:** a `bool` parameter the body uses directly as an `if` or `while` condition or a `match` scrutinee (`if flag`, `if !flag`; `if flag && x` is not read; not read in trait impls). The figures are the conservative end of common limits and are signals, not verdicts. The subject is `name` or `Type::name`; an allowlist entry for it silences all three. Not seen: macro bodies, generated code, a `bool` behind a type alias, a flag copied into a variable first, a shadowed name, a long but flat function; TypeScript and Python are not read. |
| `RDB-007` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check RDB-007`, in no profile; advisory **Info** at low confidence): one finding per production file, at its first site, with the counts and the first sites, for two shapes. **Numbers:** an integer or float literal that is an operand of an arithmetic, comparison or compound-assignment operator, an argument of a function or method call, or a bare value (a let initialiser, a return value, an assignment, a struct field, a block tail) and is not trivial (`0`, `1`, `2`, `10`, `100`, `1000`, `1024`, `255`, `60`, `24`, the powers of two up to `4096`, `0.0`, `1.0`, `0.5`; a negated trivial value is trivial). **Strings:** a string of two or more characters that appears three or more times in the file. Not read: `const` and `static` items, enum discriminants, array lengths, attributes, macro arguments (so format strings), the arguments of a tuple-struct or enum constructor, tests. The figures report only the plain cases; a literal is a cue to read, never a verdict. The subject is the file path. Not seen: a literal inside a macro, a literal copied through a variable, a string repeated across files, whether a name would help; TypeScript and Python are not read. |
| `REL-007` | any repository that tracks a project manifest (**opt-in**, run by `coderipper check REL-007`; advisory **Info**): presence only. A file named `FAILURE-MODEL*` or `failure-modes*` (and the underscore forms), or a Markdown heading about a failure model or failure modes in `ARCHITECTURE*.md` (or `architecture*.md` / `design*.md` under `docs/`), satisfies it; otherwise one finding at the manifest. One document for the repository satisfies it: the rule's per-component wording is not checked |
| `SEC-005` | Rust packages, from the source (`syn`; **opt-in**, run by `coderipper check SEC-005`; advisory **Info**): per package (the nearest `Cargo.toml`), `unsafe` blocks, `unsafe fn`s and `unsafe impl`/`unsafe trait` items are counted outside tests, examples, tools, `build.rs` and vendored trees. One finding when a block or `unsafe impl` has no `SAFETY` comment on its line or in the comment lines directly above, listing the sites, or when a package with no `unsafe` neither forbids nor denies `unsafe_code` (crate root or `[lints]`). A package whose `unsafe` is all justified, or that forbids it, says nothing. Not claimed: COR-006. Cannot see `unsafe` in macro arguments or generated code, or native code in another language |
| `SEC-007` | any repository that tracks a project manifest (**opt-in**, run by `coderipper check SEC-007`; advisory **Info**): presence only, from tracked file names. A file named `THREAT-MODEL*`, `threat-model*`, `threat_model*` or `threatmodel*` (any case), or a `threat*` file under `docs/`, satisfies it; otherwise one finding at the manifest, with the search as its control. The text of a document is never judged. A threat model kept outside the repository, or under another name, is not seen |
| `SUP-005` | Rust, from tracked `Cargo.lock` files (advisory **Info**): a package resolved to more than three distinct versions (four or more) is listed with its versions, one finding per lockfile. The limit of three is chosen from the corpus (13 of 3,173 package entries in nine lockfiles), and is a constant: a `max_versions` setting is not read yet. Cargo allows semver-incompatible versions side by side, so some duplicates cannot be removed |
| `SUP-009` | GitHub Actions workflows (`.github/workflows/*.yml`, `.yaml`; advisory **Info**): a workflow that publishes (`cargo publish`, `cargo release`, `npm publish`, `twine upload`, `gh release`, `pypa/gh-action-pypi-publish`, `softprops/action-gh-release`, `actions/create-release`, `goreleaser`; a `--dry-run` line is not read) and names no SBOM tool (`cyclonedx`, `cargo sbom`, `syft`, `spdx`, `sbom`) in a step. One finding per such workflow, at the publishing line. A release made elsewhere, or an SBOM made in a reusable workflow or composite action it calls, is not seen |
| `TYP-006` | Rust, from the source (`syn`; **opt-in**: runs when named, `coderipper check TYP-006`, in no profile): a `.unwrap()` or `.expect(..)` call outside tests with no written invariant. A site is justified by a comment on the same line, up to the end of the statement, or alone on the line above that **begins** with `INVARIANT`, `SAFETY` or `UNWRAP-OK`, or by an `expect("..")` message that states why it cannot fail (it contains `invariant`, `always`, `never`, `cannot`, `guaranteed`, `validated`, `checked` or `by construction`). One **Medium** finding per file at high confidence, at the first unjustified site, with the count. `unwrap_or`, `unwrap_or_else`, `unwrap_or_default`, `unwrap_err` and `expect_err` are not sites. Not seen: other languages, whether the receiver is a `Result`, an `Option` or a type with its own `unwrap` (`syn` has no types), calls inside macro tokens, whether a stated invariant is true. Measured on eight repositories: 517 unjustified calls in 125 files, which is why it is not default-on. |
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

`OPS-006` (no secrets in configuration files) is the same scan under its own id, narrowed to configuration files (`.yml`, `.yaml`, `.toml`,
`.ini`, `.cfg`, `.conf`, `.properties`, `.tfvars`, `.xml`, `.json` other than lockfiles, dotenv and docker-compose files): `coderipper check OPS-006`. It
has no detection of its own, so a secret in source code is `SEC-002`'s only and a secret in a configuration file is reported under both.

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
project's own zizmor ignores are honoured, as a waiver source: a committed `zizmor.yml` that disables an audit or ignores a place is
respected and reported as one finding at the file (Medium when nothing in a comment says why, Info when one does), and a finding that carries a `# zizmor: ignore` comment is reported at Info; each
says the waiver needs a reason, an owner and an expiry (GOV-002), which an `[[allow]]` entry carries.

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
project's directory. The project's own buf ignores are honoured, as a waiver source: the `breaking: ignore:` and `ignore_only:` of its tracked `buf.yaml` files
are carried into the configuration given on the command line (which still fixes the rule category), and each buf.yaml that ignores something
is one Info finding saying the waiver needs a reason, an owner and an expiry (GOV-002). One finding per violation, at the file and line buf prints, with buf's rule id as the subject.

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
