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
```

A server mode (`coderipper serve`) is planned, for a free hosted instance on
[ThinkersJournal.com](https://thinkersjournal.com) for open-source projects — not implemented yet.

## Status

Early scaffold. The host, the shared `Finding` schema, and the check-plugin interface exist; no checks are
implemented yet. See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
`docs/superpowers/plans/` for what's actually being built and in what order.

## Why

Found by hand, once, during an unrelated design review: a fully correct, well-tested module in another
project had zero callers anywhere in that project's serving path. The logic was right; nothing called it.
That's the defect class this tool exists to catch systematically, not by luck of a review going looking in
the right place.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License, Version 2.0](LICENSE-APACHE), at your
option.
