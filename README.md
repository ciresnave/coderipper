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
cargo coderipper --deny medium       # ... and exit 1 if anything is Medium or worse (what CI usually wants)
cargo coderipper --workspace         # every member of the cargo workspace

coderipper fast                      # the same, as a plain command
coderipper sweep                     # everything, including network-backed checks
coderipper check reachability        # one check by id
coderipper check unused-return-values  # is a function's return value ever consumed?
coderipper check unused-parameters   # which function parameters are never used?
coderipper check version-consistency # does every package of the project share one version?
coderipper check ci-protection-presence # does the default branch require status checks? (network; sweep tier)
```

The project must be a git repository with a commit: the checks analyse `HEAD` in a throwaway worktree and never touch
your working tree.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | every check ran and nothing is at or above `--deny` (without `--deny`, any run that finished) |
| 1 | a finding at or above `--deny` |
| 2 | usage error: an unknown flag or `--deny` level, an unknown check id, or a project path that cannot be read |
| 3 | the audit could not be completed: a check could not run (this outranks 1), or the command itself failed (for example `cache status` with the cache off) |

`--deny <info|low|medium|high|critical>` is off by default: like clippy's warnings, findings are printed but do not
fail the run, so adding CodeRipper to a project does not break its CI. A CI job that should gate writes
`cargo coderipper --deny medium`.

## Machine-readable output

`--message-format json` prints one JSON object per line on stdout, each with a `"reason"`, the way cargo's own
`--message-format json` does: a `coderipper-finding` line per finding (the `Finding` fields), then one
`coderipper-summary` line with `findings`, `errors` and `exit_code`. In this mode stdout carries only JSON.

```sh
cargo coderipper --message-format json --deny medium
```

## As a library

```toml
[dependencies]
coderipper = { version = "0.3", default-features = false }   # no clap, no binaries
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

The host, the shared `Finding` schema, and the check-plugin interface exist, and five checks are implemented:
`reachability` (dead code, including `pub` items), `unused-parameters` (a function parameter never used),
`unused-return-values` (a function whose return value every caller discards), `version-consistency` (one version per
project) and `ci-protection-presence` (the default branch requires status checks).

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

## CI and branch protection

`coderipper check ci-protection-presence` reads GitHub's branch-protection settings for the repository `origin`
points at (through `gh api`, as whichever account `gh` has active; read-only) and reports a default branch that
requires no status checks: protection missing, or enabled but with `enforcement_level` `off` or zero required
contexts. It never trusts `protected`, which is `true` on branches that enforce nothing. It is a network check, so it
runs under `coderipper sweep` or by name, never in `fast`. It cannot silently pass: no `origin`, a non-GitHub origin, an
API failure, or a reply without the `protection` object (hidden from a token without push access to a private
repository) is an error; an archived repository and one with no commits yet each get an `Info` note. To accept a
repository on purpose, use an `[[allow]]` entry with `check = "ci-protection-presence"`,
`file = "github:branch-protection"` and `symbol = "owner/repo@branch"`.

## Workspaces

A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
root that is itself a package. A virtual workspace root is refused with the list of members to choose from.
The whole repository is checked out in a throwaway worktree (so path dependencies resolve) but only the
member is rewritten and built, findings carry paths relative to the member, and `.coderipper.toml` is read
from the member's directory. The checks are project-scope: a `pub` item used only by a sibling member has no
callers within this package. Dependencies are built once and reused (see "Build cache" below).

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
and it never changes the exit code. A language in your repository that has no module is listed the same way. The default `classic`
profile prints exactly what it always has, plus one stderr line when it finds a language it has no module for (a manifest and at
least one source file among the files git tracks; a project outside git is not scanned).

## Language-neutral rules: `--profile extended`

Under `--profile extended` five rules that read a repository's files, not its source, run next to the original checks, in the same
run and under the same `.coderipper.toml` allowlist. Their ids are catalog ids; each is a narrow, deterministic reading of a broader
rule, and each is proven by a conformance fixture (`coderipper conformance --module neutral`). The default `classic` profile does
not run them. `coderipper check SUP-001 --profile extended` runs one.

| Rule | Reports |
|---|---|
| `SUP-001` | an application (a Cargo package with a binary, or a private npm package) whose lockfile is not committed; a GitHub Actions workflow that runs `npm install`, `yarn install` or `cargo build`/`test`/... without the strict mode (`npm ci`, `--frozen-lockfile`/`--immutable`, `--locked`) |
| `SUP-011` | a vendored directory (`vendor`, `third_party`, `node_modules`, ...) that a tracked analysis configuration (`codecov.yml`, `.codacy.yml`, `sonar-project.properties`, `.eslintignore`, `.deepsource.toml`) does not mention |
| `DOC-005` | a decision record (markdown in an `adr`, `adrs` or `decisions` directory) with no or an unknown status, or a supersession link that dangles, is not reciprocated, or leaves the old record current |
| `DOC-009` | a repository with a `docs/` directory but no glossary file; a glossary that defines one term twice |
| `WSP-001` | a repository with no `CODEOWNERS` file, or one with no default (`*`) owner |

Each rule's reading, and what it cannot see, is in the documentation of its module (`src/module/neutral/`). A rule that does not
apply to a repository (no decision records, no manifest it knows) is reported as not applicable here, never as clean. Files are read
from `HEAD`, so the repository must be a git repository with a commit.

## Suppressing a finding

Some findings are deliberate (public API built ahead of its consumer, a value discarded on purpose).
List them in `.coderipper.toml` at the project root. Every entry names the check, the file, and the
symbol, and **must** say why:

```toml
[[allow]]
check = "reachability"
file = "src/api.rs"
symbol = "public_entry_point"
reason = "published crate API, consumed outside this repo"
```

`symbol` is the bare name, not `Type::name`: two same-named methods in one file share an entry, so
one entry can hide a second finding with the same name (and that entry will not go stale while either
still matches). `file` is compared by path component, so `./src/api.rs` and `src\api.rs` also match; case
is not folded.

An entry that suppressed nothing in a run is reported as an `Info` finding (check id `allowlist`), so a
suppression whose cause has been fixed does not linger. An entry naming a check that does not exist is
reported the same way. An entry for a check that failed or did not run in that run is not judged.

## Why

Found by hand, once, during an unrelated design review: a fully correct, well-tested module in another
project had zero callers anywhere in that project's serving path. The logic was right; nothing called it.
That's the defect class this tool exists to catch systematically, not by luck of a review going looking in
the right place.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE), at your
option.
