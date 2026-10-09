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
coderipper = { version = "0.4", default-features = false }   # no clap, no binaries
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

Under `--profile extended` five rules that read a repository's files, not its source (and one that hands the repository to a tool,
see below) run next to the original checks, in the same
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

One finding per dead link, at the document and line it is on. A link lychee could not settle (a timeout, a failed connection, a rate
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

The lock pins gitleaks 8.30.1, osv-scanner 2.6.0, lychee 0.24.2, zizmor 1.30.1 and buf 1.73.0 for now (`tools list` shows them); each delegated rule adds its tool to the lock when it lands. A tool is installed from a checksummed single-file binary (or a `file://` copy, which
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
