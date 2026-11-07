# Package G: panics, swallowed errors, stubs and unsafe (ERR-001, TYP-006, EXT-003, SEC-005)

Read [README](README.md) first, then the rows of these rules in [`docs/rules/REMAINING.md`](../rules/REMAINING.md). Rust, source only (`syn`), per package; **neutral-module checks** (register in
`src/module/neutral/mod.rs`; copy `ops012.rs`, which already classifies binary, example, test and library code, and `efx003.rs`). Read how an existing neutral check lays out `conformance/<RULE>/`.

**ERR-001** (`rules/err.toml`, high/high): a failure result is not looked at. `unused-return-values` already reports a discarded `Result` from a call (the compiler route): do not repeat it. Report what it does
not: `let _ = fallible();` and `_ = fallible();` with no comment beside it saying why dropping is safe, `.ok();` as a statement, an `Err(_) => {}` match arm, and `if let Ok(..) = ..` with no else on a call whose
error matters (judgment: only `std::fs`, `std::io`, network and `Command` calls, from a short named list). For ERR-001 a comment on the same or the previous line **is** the stated reason the rule asks for, and
silences it (the opposite of `lint-suppression`, where a comment is deliberately not a reason: say which in the finding). Skip `Drop` impls and test code. Check `related`: ERR-011 and `unused-return-values`.

**TYP-006** (medium/high): `unwrap()` and `expect(..)` in non-test production code, except where an invariant is justified in writing (a comment beginning `INVARIANT`, `SAFETY` or `UNWRAP-OK`, or an
`expect("..")` message that states the invariant). Clippy has `unwrap_used` (restriction, off by default). This will fire widely across the portfolio: **ship opt-in** (a settings table like `[mod010]` in
`Mod010Settings`; see how `efx003.rs` was gated), report per-file counts on the corpus, and do not make it default-on. It is also the static core of ERR-003, which is not in this package.

**EXT-003** (medium/medium): `todo!()`, `unimplemented!()`, or an error saying "not supported" or "not implemented" in the body of a non-test trait impl method. A trait method with a default body is not a stub.
Syntactic: list the macro names, and the two literal phrases.

**SEC-005** (low/high): inventory of `unsafe` blocks, `unsafe fn` and `unsafe impl` per crate, flagging a block with no `// SAFETY:` comment directly above it, and telling a crate with none to add
`#![forbid(unsafe_code)]`. Info; the count goes in the detail. COR-006's static half (`unsafe` banned or each block justified) falls out of this: note it, do not claim COR-006.

Measure on the corpus. Patent read first: error-swallowing detection and unsafe inventory are old (clippy, cargo-geiger); expect lint-tool hits, not claims.

Done when: four checks registered, fixtures, corpus numbers (TYP-006 as counts only), patent rows, fragments, READY. Order: SEC-005 and EXT-003 (smallest), then ERR-001, then TYP-006.
