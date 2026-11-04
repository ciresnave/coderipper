kind: Docs
---
**CONTRIBUTING.md, docs/DIRECTORY.md, docs/FILE-FORMATS.md and the logo; the README no longer says CodeRipper audits only Rust.** The README, the
`lib.rs` crate docs and the `--help` summary now say what is true: CodeRipper audits a project in any language it holds, started with Rust, and
reports a rule that does not cover a project's language as *not run here*. The three new documents were checked field by field against
`src/finding.rs`, `src/catalog.rs`, `src/allowlist.rs` and `src/conformance.rs`. The logo master is `docs/assets/coderipper-logo-claw.png`
(1600x1600) with a `.webp` export that the README shows. The `Cargo.toml` description still says "a Rust project" (a lane may not edit it): the PM
is asked to change it when the version is stamped.
