# CodeRipper

Portfolio-wide code-integration auditor. Finds things a compiler's own lints won't tell you: functions
with no callers (including `pub` ones — `rustc`'s `dead_code` lint deliberately skips those), public APIs
nobody outside the crate actually uses, version drift across a project's crates, stale dependencies, and
more, as pluggable checks under one host.

Run it next to `cargo clippy`:

```sh
coderipper fast                      # every fast (local-only) check, current project
coderipper sweep                     # everything, including network-backed checks
coderipper check reachability        # one check by id
coderipper check unused-return-values  # is a function's return value ever consumed?
coderipper check unused-parameters   # which function parameters are never used?
coderipper check version-consistency # does every package of the project share one version?
coderipper check ci-protection-presence # does the default branch require status checks? (network; sweep tier)
```

A server mode (`coderipper serve`) is planned, for a free hosted instance on
[ThinkersJournal.com](https://thinkersjournal.com) for open-source projects — not implemented yet.

## Status

Early. The host, the shared `Finding` schema, and the check-plugin interface exist, and three checks are
implemented: `reachability` (dead code, including `pub` items), `unused-parameters` (a function parameter
never used) and `unused-return-values` (a function whose
return value every caller discards). See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
`docs/superpowers/plans/` for what's actually being built and in what order.

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
- **Fixed in 0.2.9: overlapping runs on one repository.** Before 0.2.9, two `coderipper` runs overlapping on the same repository could
  serve one run the other run's compiled copy of the package it was analysing, so it reported the other run's findings. From 0.2.9, right after
  taking the cache lock a run sets every file of its checkout to the current time, so cargo recompiles the package instead of reusing another run's
  artifact (sibling crates inside the repository were already rebuilt on every run). If a refresh is impossible the run builds without the cache and says so.
  **If you ran two overlapping runs on one repository with 0.2.7 or 0.2.8, discard the later run's findings and re-run it alone.**

## Reachability on a package with a library

`reachability` builds only the library when there is one, then does not report an item that a bin, test,
example or bench reaches by name (and everything that item reaches). It can therefore miss a dead item that
shares a name with something live, but it does not report live code as dead. `pub` items inside the bins
themselves are not analyzed when a library exists.

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
