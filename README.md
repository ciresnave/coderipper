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

## Workspaces

A project is one package. Pass a workspace member's directory (`--project fuel/fuel-core`), or a workspace
root that is itself a package. A virtual workspace root is refused with the list of members to choose from.
The whole repository is checked out in a throwaway worktree (so path dependencies resolve) but only the
member is rewritten and built, findings carry paths relative to the member, and `.coderipper.toml` is read
from the member's directory. The checks are project-scope: a `pub` item used only by a sibling member has no
callers within this package. Each run builds the member's dependencies from scratch (about two minutes for a
large crate); running every member in one command is not supported yet.

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
