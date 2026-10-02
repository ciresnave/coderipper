# Reachability on packages with a library — Implementation Plan

> **IMPLEMENTATION STATUS (2026-10-02, post-implementation, post-review): this plan was executed on branch
> `feat/reachability-lib-plus-bin` (head `1f322dc`, rebased onto `main` after #9 merged). A fresh whole-branch
> review then reproduced five classes of FALSE POSITIVES (live items reported as dead) that this plan's design
> did not cover, plus a silent over-rescue, all fixed test-first in the last commit. Kept as the historical
> record; where it disagrees with the code, trust the code.** The module docs no longer claim "never reports a
> live item as dead": they say it aims to over-rescue and list what it cannot see.
> 1. `macro_rules!` bodies were invisible: a macro is now an item named by the macro, and an item-position macro
>    invocation is a root (always compiled).
> 2. `use a as b` renames were not followed: an alias now leads back to the original name.
> 3. Bins/tests/examples/benches with a custom `path` were never scanned: `cargo metadata` supplies every
>    non-lib target and its module tree. (A custom `[lib] path` is still unsupported.)
> 4. An inherent impl's header (bounds, where clauses) was dropped: each method now carries it (without becoming
>    live by itself).
> 5. Path-shaped string literals (`#[serde(default = "f")]`) now name what they point at.
> 6. Keywords (`impl`, `for`) and the impl's own generic parameters made EVERY trait impl live as soon as anything
>    reachable mentioned them. Keywords are no longer identifiers, and a trait impl is live with the lib type it
>    implements (or, for a foreign/generic self type, its trait) — the rule in "What running it taught" item 3 was
>    too loose.
> 7. When every candidate is set aside the run printed "no issues found"; it now emits an Info finding saying so.
> Also: non-UTF-8 files no longer fail the scan. Measured on `fuel-core` (read-only clone, 40 src files, 35
> integration tests): 308 dead-in-lib candidates -> 102 reported, 206 set aside; the first draft (before these fixes)
> reported 143, i.e. 48 of those were live code.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the reachability check work on a Rust package that has a library target consumed by its own
bin, integration tests, examples or benches — today it refuses (`E0603`), which is why CodeRipper cannot
scan itself.

**Architecture:** The check downgrades every `pub` to `pub(crate)` and lets rustc's `dead_code` lint report.
That breaks every *other* target that imports the lib by name. So when `src/lib.rs` exists, build **only the
library**, and decline to report a candidate that one of the other targets reaches: start from the identifiers
the files the lib does not compile mention (bins, tests, examples, benches, bin-only modules), and follow
identifier mentions through every item of the lib to a fixpoint ("rescue"). Name-based, so it can only
over-rescue (miss a finding), never report a live item as dead. Packages with no lib are analyzed exactly as before.

**Tech Stack:** Rust 2021, no new dependencies (`syn`, `proc-macro2` with `span-locations` are in `Cargo.toml`).

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §3 (Sub-pass A). Approach chosen by the PM
(2026-10-02) from three options: **name-based rescue over a lib-only build**; the exact alternative (tag uses with
`#[deprecated]`, second build) was rejected because its worst failure is a false positive.
PM sub-rulings: (a) name-based over-rescue is acceptable v1 precision; (b) `pub` items inside bin targets stay out
of scope; (c) moving reachability's build/parse onto `cargo_json` + `--cap-lints=warn` rides along.

**Stacked on milestone B.** This branch uses `cargo_json::{build_all_targets(root, flags), CAP_LINTS, compose_rustflags}`
from the unused-parameters PR (#9). `git fetch origin`: if #9 is merged, branch from `origin/main`; otherwise from
`origin/feat/unused-parameters`, and after #9 merges rebase this branch onto `main`
(`git rebase --onto origin/main <old base tip>`; the portfolio's repos squash-merge).

## What running it taught (found by building and probing, not by reading)

1. **Reproduced the refusal:** a lib consumed by its own bin, with the lib's `pub` downgraded, gives six
   `E0603` under `--all-targets`; `cargo build --lib` on the same tree builds fine and reports `dead_code` for
   lib items. Also: `pub` items of a *bin* crate are not reported by rustc unless downgraded (hence decision 4).
2. **Following only the candidates is not enough — the acceptance probe caught it.** The first version
   propagated "rescued" only from one reported item to another. Run on CodeRipper itself it printed **77
   findings, nearly all live code**: the path from the bin to a helper passes through items rustc does *not*
   report (a trait impl's method body, any live intermediate function). The graph must cover **every** item of
   the lib. A regression test with exactly that shape (`a_path_from_the_bin_through_a_trait_impl_keeps_everything_it_reaches_alive`) failed with a false positive before the fix.
3. **A trait impl is live with its type, but an inherent method is not.** Nobody mentions `drop`/`fmt`/`default` by
   name, so an `impl Trait for Type` block must become reachable as soon as its trait or type is; otherwise what its
   methods call is reported as dead. The same rule for inherent `impl Type { fn m }` would hide every dead method
   of a live type — the most common real finding — so there each method must be mentioned itself.
4. **A name inside a format string is a use.** `format!("{NAME}")` and thiserror's `#[error("... {NAME}")]` use
   `NAME`, but a lexer sees one opaque string. After fix 2 the probe still printed three false positives, all of
   this kind (`SENTINEL_FN` twice, `ABSENCE_CLAIM_HELP`). The identifier scan now reads `{word` inside string literals.
5. After both fixes: **CodeRipper scanned by itself reports nothing**, and three planted dead items (a fn, a struct,
   a method) are all found.

## Design decisions (rulings, with what each costs if wrong)

1. **`has_lib` = `src/lib.rs` exists.** A custom `[lib] path` is not followed (documented). *Cost:* such a lib is
   treated as no-lib and fails as it does today (loudly).
2. **The foreign set** = every `.rs` under `tests/`, `examples/`, `benches/` and `src/` that the lib's module tree
   does not contain (`src/main.rs`, `src/bin/**`, modules only a bin declares). The lib's own files are *not* foreign:
   a dead item in `src/inner.rs` is not rescued by its own file. The lib tree follows `mod x;` the way rustc does
   (`x.rs`, `x/mod.rs`, non-mod-rs `foo/`, inline `mod a { mod b; }`, `#[path]`); a declared module with no file is
   skipped (`cfg`'d out); an unparseable file *inside* the lib tree is an error. *Cost if wrong:* a `cfg(test)`
   module is counted as lib, so an item used only by it is still reported — the existing, documented
   "called only from a test" gap, unchanged.
3. **Identifiers are lexed (comments and strings ignored, except `{word` captures); text that does not lex falls
   back to every word.** Over-approximating on purpose, and it means a template file under `src/` cannot make the
   check fail.
4. **`pub` items inside bin targets are not analyzed when a lib exists** (PM ruling b).
5. **`--cap-lints=warn` for reachability too** (Task 1): `#![deny(warnings)]` / `[lints]` turned an unrelated
   lint into an "error" the old parser mistook for a broken build, failing the whole check. A test pins it.
6. **A real compile error is still an error**, as is a crate-wide `#![allow(dead_code)]` (the sentinel). The sentinel
   stays in `src/lib.rs` (the lib is the only crate built).
7. **Findings say when candidates were set aside**: the detail adds "N other candidate(s) ... not reported because
   a file the library does not compile mentions their name", so a missing finding is explainable.

## Global Constraints

- Rust edition 2021; CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --all-targets`, `cargo test --no-fail-fast` on **ubuntu, windows and macos**. The branch tip must
  pass all four (intermediate commits are not clippy-gated: Task 1 adds an enum variant nothing constructs until Task 4).
- **A test must never point a check at the shared checkout** (`git worktree add` writes into the repo's `.git`).
  Use throwaway fixtures: `tests/common/mod.rs::git_repo_with`.
- A finding that claims an absence must carry a `positive_control` (the sentinel). A failed control returns `Err`,
  never `Ok(vec![])`.
- `Check::run` returns RAW findings; the host applies the allowlist. `Finding::subject` stays the bare symbol name.
- Do **not** bump any version in `Cargo.toml`: the portfolio PM allocates it at gate time.
- Clone with `-c core.autocrlf=false` on this Windows box; never `checkout` in a shared tree.
- Backslash literals via a shell heredoc can be dropped or doubled here — use the editor/Write tool. In Python helper
  scripts `\a` becomes a BEL character.
- Commit trailers (this lane): end each commit message with
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. Do not run `gh auth switch` (the lane
  account cannot open or merge PRs here; the PM does).

## Review Focus

1. **A path that passes through an item rustc did not report** (trait impl body, live helper) must keep what it
   reaches alive. → `the_path_may_pass_through_an_item_that_is_not_a_candidate` (Task 3), `a_path_from_the_bin_through_a_trait_impl_keeps_everything_it_reaches_alive` (Task 4).
2. **Trait impls are live with their type; inherent methods are not.** → `a_trait_impl_is_live_with_its_type_so_what_its_methods_call_is_reachable`, `an_unreached_trait_impl_does_not_rescue_what_it_calls`, `a_dead_inherent_method_of_a_reached_type_is_still_reported` (Task 3).
3. **Names inside format strings and macro arguments are uses.** → `a_name_captured_inside_a_format_string_counts` (Task 2), `a_constant_used_only_through_a_format_string_capture_is_alive` (Task 4).
4. **Module-tree resolution**: nested, `mod.rs`, non-mod-rs, inline, `#[path]`, absent file, and the lib's own files not being foreign. → `module_files_are_resolved_the_way_rustc_does`, `a_file_the_lib_does_not_declare_is_foreign_and_so_are_tests_examples_and_benches` (Task 2), `a_mention_inside_the_lib_itself_does_not_rescue`, `integration_tests_examples_benches_src_bin_and_bin_only_modules_all_rescue` (Task 4).
5. **The accepted loss is visible, not silent**: over-rescue on a shared name is pinned, and a finding says when candidates were set aside. → `a_name_shared_with_something_the_bin_uses_is_rescued_that_is_the_accepted_over_rescue` (Task 4).

**Known gaps accepted for v1 (documented):** `pub` items in bin targets; a custom `[lib] path`; a lib item used only
by a `#[cfg(test)]` module of the lib; identifiers produced by macros (`concat_idents`, `paste`); a dead item whose
name is shared with anything reachable.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/checks/reachability/diagnostics.rs` | Collect `dead_code` hits through `cargo_json`; `Targets::{All, LibOnly}` |
| `src/cargo_json.rs` | + `build_lib_only` |
| `src/checks/reachability/foreign.rs` (new) | The lib's module tree; identifiers of every other file |
| `src/checks/reachability/rescue.rs` (new) | Name reachability over the whole lib; which candidates to set aside |
| `src/checks/reachability/mod.rs` | Choose targets; apply rescue; note it in the finding |
| `tests/reachability.rs` | End-to-end fixtures (replaces the test that pinned the refusal) |

Baseline before starting (branch tip of #9, `origin/feat/unused-parameters` at `e1c89a2`): `cargo test` shows 87
lib tests, 8 in `tests/allowlist.rs`, 8 in `tests/cli.rs`, 4 in `tests/reachability.rs`, 11 in
`tests/unused_parameters.rs`, 13 in `tests/unused_return_values.rs`.

---

### Task 1: Reachability builds and parses through `cargo_json`

**Files:** Modify `src/cargo_json.rs`, `src/checks/reachability/diagnostics.rs`.

**Interfaces:**
- Produces: `cargo_json::build_lib_only(root: &Path, extra_rustflags: &str) -> anyhow::Result<BuildOutput>` (`cargo build --lib`);
  `diagnostics::Targets::{All, LibOnly}`; `diagnostics::collect_dead_code_in(root: &Path, targets: Targets) -> anyhow::Result<CollectResult>`
  (and a `collect_dead_code(root)` wrapper for `Targets::All`, removed again in Task 4). `CollectResult.build_failed_for_other_reasons` now means `BuildOutput::is_broken()`.
  The old private JSON structs, the path-separator helper and its test are gone (`cargo_json` normalizes and tests that).

- [ ] **Step 1: Write the failing test** in `diagnostics.rs`'s `mod tests` (it needs no new API):

```rust
    #[test]
    fn another_denied_lint_is_not_mistaken_for_a_broken_build() {
        // `#![deny(warnings)]` turns EVERY lint into an error, e.g. an unused import. The old parser
        // treated any non-dead_code error as "the build is broken" and made the whole check error;
        // `--cap-lints=warn` keeps them warnings, so only a real compiler error counts.
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(
            tmp.path(),
            "#![deny(warnings)]\nuse std::collections::HashMap;\npub(crate) fn dead() {}\n",
        );

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code(tmp.path()).unwrap();

        assert!(!build_failed_for_other_reasons);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].symbol, "dead");
    }
```

- [ ] **Step 2: Run it.** Run: `cargo test --lib another_denied`
  Expected: FAIL — `assertion failed: !build_failed_for_other_reasons` (the real, current bug: the unused import under `deny(warnings)` is an error the old parser counted as a broken build).

- [ ] **Step 3: Implement.**

````diff
diff --git a/src/cargo_json.rs b/src/cargo_json.rs
--- a/src/cargo_json.rs
+++ b/src/cargo_json.rs
@@ -131,9 +131,19 @@ pub(crate) fn compose_rustflags(existing: &str, extra: &str) -> String {
 /// is appended to the caller's `RUSTFLAGS`; `CARGO_ENCODED_RUSTFLAGS` is removed because cargo prefers
 /// it and it would drop the flags.
 pub(crate) fn build_all_targets(root: &Path, extra_rustflags: &str) -> anyhow::Result<BuildOutput> {
+    build_with(root, "--all-targets", extra_rustflags)
+}
+
+/// Like [`build_all_targets`] but only the library target: bins, tests, examples and benches are not
+/// compiled, so a change that breaks them (e.g. downgrading the lib's `pub` items) cannot fail it.
+pub(crate) fn build_lib_only(root: &Path, extra_rustflags: &str) -> anyhow::Result<BuildOutput> {
+    build_with(root, "--lib", extra_rustflags)
+}
+
+fn build_with(root: &Path, target_arg: &str, extra_rustflags: &str) -> anyhow::Result<BuildOutput> {
     let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
     let output = Command::new("cargo")
-        .args(["build", "--all-targets", "--message-format=json"])
+        .args(["build", target_arg, "--message-format=json"])
         .current_dir(root)
         .env("RUSTFLAGS", compose_rustflags(&existing, extra_rustflags))
         .env_remove("CARGO_ENCODED_RUSTFLAGS")
diff --git a/src/checks/reachability/diagnostics.rs b/src/checks/reachability/diagnostics.rs
--- a/src/checks/reachability/diagnostics.rs
+++ b/src/checks/reachability/diagnostics.rs
@@ -1,6 +1,5 @@
-use serde::Deserialize;
+use crate::cargo_json::{build_all_targets, build_lib_only, CAP_LINTS};
 use std::path::Path;
-use std::process::Command;
 
 pub struct DeadCodeHit {
     pub file: String,
@@ -13,98 +12,57 @@ pub struct CollectResult {
     pub build_failed_for_other_reasons: bool,
 }
 
-#[derive(Deserialize)]
-struct CargoMessage {
-    reason: String,
-    message: Option<CompilerMessage>,
+/// Which targets to build. `LibOnly` is for a package with a library: downgrading the lib's `pub`
+/// items breaks every OTHER target that imports it by crate name (E0603), so those must not be built.
+#[derive(Clone, Copy, PartialEq, Eq, Debug)]
+pub enum Targets {
+    All,
+    LibOnly,
 }
 
-#[derive(Deserialize)]
-struct CompilerMessage {
-    code: Option<CompilerCode>,
-    level: String,
-    message: String,
-    spans: Vec<CompilerSpan>,
+pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult> {
+    collect_dead_code_in(worktree_root, Targets::All)
 }
 
-#[derive(Deserialize)]
-struct CompilerCode {
-    code: String,
-}
-
-#[derive(Deserialize)]
-struct CompilerSpan {
-    file_name: String,
-    line_start: u32,
-    is_primary: bool,
-}
-
-pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult> {
-    let output = Command::new("cargo")
-        .args(["build", "--all-targets", "--message-format=json"])
-        .current_dir(worktree_root)
-        .output()?;
+pub fn collect_dead_code_in(
+    worktree_root: &Path,
+    targets: Targets,
+) -> anyhow::Result<CollectResult> {
+    // `--cap-lints=warn`: a `#![deny(warnings)]` or `[lints]` table must not turn unrelated lints
+    // into errors that this would mistake for "the build is broken" (and must not stop cargo
+    // compiling dependent targets). dead_code stays a warning, which is all this needs.
+    let build = match targets {
+        Targets::All => build_all_targets(worktree_root, CAP_LINTS)?,
+        Targets::LibOnly => build_lib_only(worktree_root, CAP_LINTS)?,
+    };
 
     let mut hits = Vec::new();
-    let mut saw_error = false;
-
-    for line in String::from_utf8_lossy(&output.stdout).lines() {
-        let Ok(msg) = serde_json::from_str::<CargoMessage>(line) else {
-            continue;
-        };
-        if msg.reason != "compiler-message" {
-            continue;
-        }
-        let Some(cm) = msg.message else { continue };
-
-        // Check dead_code FIRST, regardless of level: a crate-level `#![deny(warnings)]` promotes
-        // dead_code to `error`, and that's still a real, reportable finding -- arguably a more
-        // urgent one, since it's actively breaking that crate's own build. Only a non-dead_code
-        // error means something else is genuinely broken.
-        let is_dead_code = cm
-            .code
-            .as_ref()
-            .map(|c| c.code == "dead_code")
-            .unwrap_or(false);
-
-        if is_dead_code {
-            // Grouped diagnostics ("methods `a`, `b`, and `c` are never used") carry one PRIMARY
-            // span per symbol, in the same order as the backtick-quoted names in the message text.
-            // Taking only the first (as an earlier version of this function did) silently lost
-            // every symbol after the first in any grouped diagnostic.
-            let names = extract_symbol_names(&cm.message);
-            let primary_spans: Vec<&CompilerSpan> =
-                cm.spans.iter().filter(|s| s.is_primary).collect();
-            for (span, name) in primary_spans.iter().zip(names.iter()) {
-                hits.push(DeadCodeHit {
-                    file: normalize_path_separators(&span.file_name),
-                    line: span.line_start,
-                    symbol: name.clone(),
-                });
-            }
+    for d in &build.diagnostics {
+        if d.code.as_deref() != Some("dead_code") {
             continue;
         }
-
-        if cm.level == "error" {
-            saw_error = true;
+        // Grouped diagnostics ("methods `a`, `b`, and `c` are never used") carry one PRIMARY
+        // span per symbol, in the same order as the backtick-quoted names in the message text.
+        // Taking only the first silently loses every symbol after it.
+        let names = extract_symbol_names(&d.message);
+        let primary_spans: Vec<_> = d.spans.iter().filter(|s| s.is_primary).collect();
+        for (span, name) in primary_spans.iter().zip(names.iter()) {
+            hits.push(DeadCodeHit {
+                file: span.file.clone(),
+                line: span.line,
+                symbol: name.clone(),
+            });
         }
     }
 
-    // `--all-targets` compiles the crate more than once (the plain lib, and again for the test
-    // harness binary), so the same dead_code site is reported once per compilation. Dedupe by
+    // The same dead_code site is reported once per compilation of the shared code; dedupe by
     // (file, line, symbol) -- that triple uniquely identifies one diagnostic site.
     hits.sort_by(|a, b| (&a.file, a.line, &a.symbol).cmp(&(&b.file, b.line, &b.symbol)));
     hits.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.symbol == b.symbol);
 
-    // A build can fail with NO compiler-message at all (e.g. a panicking build.rs writes plain
-    // text to stderr, not rustc JSON) -- `saw_error` would stay false even though the build
-    // genuinely failed. A non-zero exit with nothing informative parsed is exactly that case.
-    let build_failed_for_other_reasons =
-        saw_error || (!output.status.success() && hits.is_empty() && !saw_error);
-
     Ok(CollectResult {
         hits,
-        build_failed_for_other_reasons,
+        build_failed_for_other_reasons: build.is_broken(),
     })
 }
 
@@ -120,26 +78,11 @@ fn extract_symbol_names(message: &str) -> Vec<String> {
         .collect()
 }
 
-/// rustc's JSON `file_name` uses the platform-native separator; every downstream consumer (the
-/// allowlist, Finding.location, CLI display) expects the portable forward-slash form.
-fn normalize_path_separators(path: &str) -> String {
-    path.replace('\\', "/")
-}
-
 #[cfg(test)]
 mod tests {
-    use super::{collect_dead_code, normalize_path_separators, CollectResult};
+    use super::{collect_dead_code, CollectResult};
     use std::path::Path;
 
-    #[test]
-    fn windows_backslashes_are_normalized_to_forward_slashes() {
-        // rustc's own JSON `file_name` uses the platform-native separator (confirmed empirically on
-        // Windows: "src\\main.rs", not "src/main.rs") -- the allowlist's portable, TOML-author-facing
-        // convention is forward slashes, so this must be normalized at the source.
-        assert_eq!(normalize_path_separators("src\\main.rs"), "src/main.rs");
-        assert_eq!(normalize_path_separators("src/main.rs"), "src/main.rs");
-    }
-
     fn write_fixture(dir: &Path, lib_rs: &str) {
         std::fs::write(
             dir.join("Cargo.toml"),
@@ -228,6 +171,27 @@ mod tests {
         assert_eq!(hits[0].symbol, "dead");
     }
 
+    #[test]
+    fn another_denied_lint_is_not_mistaken_for_a_broken_build() {
+        // `#![deny(warnings)]` turns EVERY lint into an error, e.g. an unused import. The old parser
+        // treated any non-dead_code error as "the build is broken" and made the whole check error;
+        // `--cap-lints=warn` keeps them warnings, so only a real compiler error counts.
+        let tmp = tempfile::tempdir().unwrap();
+        write_fixture(
+            tmp.path(),
+            "#![deny(warnings)]\nuse std::collections::HashMap;\npub(crate) fn dead() {}\n",
+        );
+
+        let CollectResult {
+            hits,
+            build_failed_for_other_reasons,
+        } = collect_dead_code(tmp.path()).unwrap();
+
+        assert!(!build_failed_for_other_reasons);
+        assert_eq!(hits.len(), 1);
+        assert_eq!(hits[0].symbol, "dead");
+    }
+
     #[test]
     fn a_build_rs_panic_with_no_compiler_message_is_still_flagged_as_a_failure() {
         // Review finding: a panicking build.rs writes plain text to stderr, not rustc JSON --
````

- [ ] **Step 4: Run the whole suite.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 87 lib (+1 new, −1 removed path-separator test), every other count unchanged (8, 8, 4, 11, 13). The six existing `diagnostics` tests passing proves the move kept the parser's behaviour (grouped diagnostics, `deny(warnings)` dead_code, a panicking `build.rs`, a syntax error).

- [ ] **Step 5: Commit.** Expect one `dead_code` warning (`LibOnly` is never constructed yet); do not clippy-gate this commit.
  `git add -A src && git commit -m "refactor(reachability): build and parse through cargo_json with --cap-lints=warn"` (+ trailers).

---

### Task 2: The lib's module tree and the other targets' identifiers

**Files:** Create `src/checks/reachability/foreign.rs`; modify `src/checks/reachability/mod.rs` (declare it).

**Interfaces:**
- Produces: `foreign::lib_module_files(root: &Path) -> anyhow::Result<BTreeSet<String>>` (`src/...` paths, forward slashes, starting at `src/lib.rs`);
  `foreign::foreign_identifiers(root: &Path, lib_files: &BTreeSet<String>) -> anyhow::Result<BTreeSet<String>>`;
  `foreign::identifiers_in(source: &str) -> BTreeSet<String>`.
  Uses the existing `worktree::walk_rs_files`.

- [ ] **Step 1: Declare the module.** In `src/checks/reachability/mod.rs`, add `mod foreign;` after `mod diagnostics;`.

- [ ] **Step 2: Write the tests first.** Create `src/checks/reachability/foreign.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from the full file in Step 3. Run: `cargo test --lib foreign`
  Expected: FAIL to compile — `cannot find function lib_module_files / foreign_identifiers / identifiers_in`.

- [ ] **Step 3: Replace the file with the full implementation:**

````rust
// src/checks/reachability/foreign.rs
//! Which files does the LIBRARY compile, and which identifiers do every OTHER target's files mention?
//!
//! The reachability check downgrades the lib's `pub` items and builds only the lib (a bin or test
//! importing the lib by name would fail to compile against the downgraded items). So a lib item used
//! only by a bin, an integration test, an example or a bench looks dead to that build. This module
//! finds the files that are NOT part of the lib, and the identifiers they mention, so the caller can
//! decline to report an item one of them names ("rescue"). Name-based on purpose: it can only
//! over-rescue (hide a dead item that shares a name with something used elsewhere), never report a
//! live item as dead.

use std::collections::BTreeSet;
use std::path::Path;

/// Every file under `root` that `src/lib.rs` compiles, following `mod x;` declarations the way
/// rustc does (`x.rs`, `x/mod.rs`, a non-mod-rs file's own directory, inline `mod a { mod b; }`,
/// `#[path = "..."]`). Paths are `src/...` with forward slashes. A declared module whose file does
/// not exist is skipped (it may be `cfg`'d out). A file that is in the lib's tree but cannot be
/// parsed is an error: the lib must compile, so this is a bug in `syn`'s coverage, not a skip.
pub fn lib_module_files(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let mut walk = Walk {
        root,
        seen: BTreeSet::new(),
    };
    walk.file("src/lib.rs", true)?;
    Ok(walk.seen)
}

struct Walk<'a> {
    root: &'a Path,
    seen: BTreeSet<String>,
}

impl Walk<'_> {
    /// `is_mod_rs`: the file is a crate root or a `mod.rs`, so its child modules live next to it;
    /// any other file `foo.rs` keeps its child modules in `foo/`.
    fn file(&mut self, rel: &str, is_mod_rs: bool) -> anyhow::Result<()> {
        if !self.seen.insert(rel.to_string()) {
            return Ok(());
        }
        let source = std::fs::read_to_string(self.root.join(rel))?;
        let parsed = syn::parse_file(&source)
            .map_err(|e| anyhow::anyhow!("could not parse {rel} with syn ({e})"))?;
        let dir = parent(rel);
        let mod_dir = if is_mod_rs {
            dir.clone()
        } else {
            join(&dir, file_stem(rel))
        };
        self.items(&parsed.items, &dir, &mod_dir, false)
    }

    fn items(
        &mut self,
        items: &[syn::Item],
        file_dir: &str,
        mod_dir: &str,
        inside_inline: bool,
    ) -> anyhow::Result<()> {
        for item in items {
            let syn::Item::Mod(m) = item else { continue };
            let name = m.ident.to_string();
            let name = name.trim_start_matches("r#").to_string();
            let path_attr = path_attribute(&m.attrs);

            if let Some((_, inner)) = &m.content {
                // `mod a { ... }`: children live in a directory named after it (or its #[path]).
                let component = path_attr.unwrap_or(name);
                let child_dir = join(mod_dir, &component);
                self.items(inner, file_dir, &child_dir, true)?;
                continue;
            }

            // `mod a;`: a #[path] outside any inline module is relative to the declaring file's
            // directory; inside one it is relative to the module directory.
            let candidates: Vec<(String, bool)> = match path_attr {
                Some(p) => {
                    let base = if inside_inline { mod_dir } else { file_dir };
                    vec![(join(base, &p), true)]
                }
                None => vec![
                    (join(mod_dir, &format!("{name}.rs")), false),
                    (join(&join(mod_dir, &name), "mod.rs"), true),
                ],
            };
            if let Some((rel, is_mod_rs)) = candidates
                .into_iter()
                .find(|(rel, _)| self.root.join(rel).is_file())
            {
                self.file(&rel, is_mod_rs)?;
            }
        }
        Ok(())
    }
}

fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|a| {
        if !a.path().is_ident("path") {
            return None;
        }
        match &a.meta.require_name_value().ok()?.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s.value()),
            _ => None,
        }
    })
}

fn parent(rel: &str) -> String {
    rel.rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default()
}

fn file_stem(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.strip_suffix(".rs").unwrap_or(name)
}

fn join(dir: &str, name: &str) -> String {
    // `a/b/../c` is left as written: the OS resolves it when the file is read.
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Every identifier mentioned by a `.rs` file that is not part of the lib: all of `tests/`,
/// `examples/` and `benches/`, plus every file under `src/` that `lib_files` does not contain
/// (`src/main.rs`, `src/bin/**`, and modules only a bin declares).
pub fn foreign_identifiers(
    root: &Path,
    lib_files: &BTreeSet<String>,
) -> anyhow::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for top in ["src", "tests", "examples", "benches"] {
        let dir = root.join(top);
        if !dir.is_dir() {
            continue;
        }
        for path in crate::worktree::walk_rs_files(&dir)? {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            if lib_files.contains(&rel) {
                continue;
            }
            out.extend(identifiers_in(&std::fs::read_to_string(&path)?));
        }
    }
    Ok(out)
}

/// Identifiers in `source`. Lexed, so comments and string literals do not count; if the text does
/// not lex (a template file, syntax this lexer predates) every identifier-looking word counts
/// instead -- over-approximating, which is the safe direction here.
pub fn identifiers_in(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    match source.parse::<proc_macro2::TokenStream>() {
        Ok(tokens) => collect(tokens, &mut out),
        Err(_) => scan_words(source, &mut out),
    }
    out
}

fn collect(tokens: proc_macro2::TokenStream, out: &mut BTreeSet<String>) {
    for tree in tokens {
        match tree {
            proc_macro2::TokenTree::Ident(i) => {
                out.insert(i.to_string().trim_start_matches("r#").to_string());
            }
            proc_macro2::TokenTree::Group(g) => collect(g.stream(), out),
            proc_macro2::TokenTree::Literal(l) => format_captures(&l.to_string(), out),
            _ => {}
        }
    }
}

/// `{name}` / `{name:?}` inside a string literal is a use of `name` (inline format arguments, and
/// thiserror's `#[error("...")]`). Any word right after a `{` counts; a literal with no `{` adds
/// nothing, and `{{` escapes only over-approximate.
fn format_captures(literal: &str, out: &mut BTreeSet<String>) {
    for part in literal.split('{').skip(1) {
        let word: String = part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if word.chars().next().is_some_and(|f| !f.is_ascii_digit()) {
            out.insert(word);
        }
    }
}

fn scan_words(source: &str, out: &mut BTreeSet<String>) {
    let mut word = String::new();
    for c in source.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            if word.chars().next().is_some_and(|f| !f.is_ascii_digit()) {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, contents) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        tmp
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn module_files_are_resolved_the_way_rustc_does() {
        let tmp = tree(&[
            (
                "src/lib.rs",
                "mod a;\nmod b { mod c; }\n#[path = \"x/y.rs\"]\nmod z;\nmod dir;\nmod absent;\n",
            ),
            ("src/a.rs", "mod aa;\n"),
            ("src/a/aa.rs", ""),
            ("src/b/c.rs", ""),
            ("src/x/y.rs", ""),
            ("src/dir/mod.rs", "mod leaf;\n"),
            ("src/dir/leaf.rs", ""),
            // not declared by the lib: belongs to a bin
            ("src/main.rs", "mod util;\nfn main() {}\n"),
            ("src/util.rs", ""),
        ]);
        assert_eq!(
            lib_module_files(tmp.path()).unwrap(),
            set(&[
                "src/lib.rs",
                "src/a.rs",
                "src/a/aa.rs",
                "src/b/c.rs",
                "src/x/y.rs",
                "src/dir/mod.rs",
                "src/dir/leaf.rs",
            ])
        );
    }

    #[test]
    fn a_file_the_lib_does_not_declare_is_foreign_and_so_are_tests_examples_and_benches() {
        let tmp = tree(&[
            ("src/lib.rs", "mod inner;\npub fn lib_only() {}\n"),
            ("src/inner.rs", "pub fn inside_lib() {}\n"),
            ("src/main.rs", "mod util;\nfn main() { from_main(); }\n"),
            ("src/util.rs", "fn from_util() {}\n"),
            ("src/bin/tool.rs", "fn main() { from_bin(); }\n"),
            ("tests/t.rs", "fn t() { from_test(); }\n"),
            ("examples/e.rs", "fn main() { from_example(); }\n"),
            ("benches/b.rs", "fn b() { from_bench(); }\n"),
        ]);
        let lib = lib_module_files(tmp.path()).unwrap();
        let foreign = foreign_identifiers(tmp.path(), &lib).unwrap();
        for name in [
            "from_main",
            "from_util",
            "from_bin",
            "from_test",
            "from_example",
            "from_bench",
        ] {
            assert!(
                foreign.contains(name),
                "{name} should be foreign: {foreign:?}"
            );
        }
        // the lib's own files are not foreign: a lib item mentioned only inside the lib is not rescued
        assert!(!foreign.contains("lib_only") && !foreign.contains("inside_lib"));
    }

    #[test]
    fn identifiers_come_from_code_not_comments_or_strings() {
        let ids = identifiers_in(
            "// in_comment\nfn real() { let s = \"in_string\"; call!(in_macro); r#type(); }\n",
        );
        assert!(ids.contains("real") && ids.contains("in_macro") && ids.contains("type"));
        assert!(!ids.contains("in_comment") && !ids.contains("in_string"));
    }

    #[test]
    fn a_name_captured_inside_a_format_string_counts() {
        // `format!("{NAME}")` uses NAME exactly as `format!("{}", NAME)` does, and so does a
        // thiserror `#[error("... {NAME}")]`. A self-scan of CodeRipper reported two live constants
        // as dead because the lexer treats the string as opaque.
        let ids =
            identifiers_in("fn f() { println!(\"{captured} and {spec:?} and {{escaped}}\"); }");
        assert!(ids.contains("captured") && ids.contains("spec"), "{ids:?}");
        // an ordinary string with no braces still contributes nothing
        assert!(!identifiers_in("fn f() { let s = \"plain words here\"; }").contains("plain"));
    }

    #[test]
    fn text_that_does_not_lex_still_yields_every_word() {
        // A template file under src/: not Rust, but it must neither fail the check nor be ignored.
        let ids = identifiers_in("fn {{name}}() { \"unterminated\n some_word");
        assert!(ids.contains("some_word") && ids.contains("name"), "{ids:?}");
    }

    #[test]
    fn an_unparseable_lib_file_is_an_error_not_a_silent_skip() {
        let tmp = tree(&[("src/lib.rs", "this is not rust {{{")]);
        assert!(lib_module_files(tmp.path()).is_err());
    }
}
````

- [ ] **Step 4: Run the tests.** Run: `cargo test --lib foreign`
  Expected: PASS, 6 tests. (The format-string test is Review Focus 3; it was added after the Task 4 self-scan probe failed — see "What running it taught", item 4 — and is listed here because the final file carries it.)

- [ ] **Step 5: Prove two guards can fail.** (a) In `file()`, replace `join(&dir, file_stem(rel))` with `dir.clone()`: `module_files_are_resolved_the_way_rustc_does` must FAIL (children of a non-mod-rs file live in `foo/`). (b) In `foreign_identifiers`, delete the `if lib_files.contains(&rel) { continue; }` block: `a_file_the_lib_does_not_declare_is_foreign_and_so_are_tests_examples_and_benches` must FAIL. Revert each.

- [ ] **Step 6: Format and commit.** `cargo fmt && git add -A src && git commit -m "feat(reachability): find the lib module tree and the identifiers every other target mentions"`. (Not clippy-gated: unused until Task 4.)

---

### Task 3: Name reachability over the whole lib

**Files:** Create `src/checks/reachability/rescue.rs`; modify `src/checks/reachability/mod.rs` (declare it).

**Interfaces:**
- Consumes: `diagnostics::DeadCodeHit { file, line, symbol }`, `foreign::identifiers_in`.
- Produces: `rescue::rescue(hits: Vec<DeadCodeHit>, foreign: &BTreeSet<String>, root: &Path, lib_files: &BTreeSet<String>) -> anyhow::Result<Split>` with `Split { kept: Vec<DeadCodeHit>, rescued: Vec<DeadCodeHit> }`, order preserved.

- [ ] **Step 1: Declare the module.** Add `mod rescue;` after `mod foreign;` in `src/checks/reachability/mod.rs`.

- [ ] **Step 2: Write the tests first.** Create `src/checks/reachability/rescue.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from Step 3. Run: `cargo test --lib rescue`
  Expected: FAIL to compile — `cannot find function rescue / type DeadCodeHit`.

- [ ] **Step 3: Replace the file with the full implementation:**

````rust
// src/checks/reachability/rescue.rs
//! Decides which dead-code candidates NOT to report because a target the lib build never compiled
//! (a bin, a test, an example, a bench) reaches them.
//!
//! Name-based reachability over the WHOLE library. Start from the identifiers the foreign files
//! mention; every item of the lib that carries one of those names makes the identifiers in its own
//! source reachable too, and so on to a fixpoint. A candidate is rescued when its name is reachable.
//!
//! It must run over every item, not only over the candidates: the path from a bin to a candidate
//! usually passes through items rustc did not report (a trait impl's method body, say).
//!
//! Two rules keep it sound in the direction that matters (never report a live item as dead):
//! - an `impl Trait for Type` block is live as soon as its header (the trait or the type) is
//!   reachable, so every identifier in the block becomes reachable. rustc never reports a trait
//!   impl's methods, and nothing calls `drop`/`fmt`/`default` by name;
//! - inherent `impl Type { fn m }` is NOT treated that way: `m` itself must be mentioned, otherwise a
//!   dead method of a live type would never be reported.
//!
//! Names, not paths: it can over-rescue (a dead `new` hidden because something reachable mentions
//! `new`), never under-rescue.

use super::diagnostics::DeadCodeHit;
use super::foreign::identifiers_in;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;

pub struct Split {
    pub kept: Vec<DeadCodeHit>,
    pub rescued: Vec<DeadCodeHit>,
}

/// `lib_files`: every file the lib compiles (`foreign::lib_module_files`).
pub fn rescue(
    hits: Vec<DeadCodeHit>,
    foreign: &BTreeSet<String>,
    root: &Path,
    lib_files: &BTreeSet<String>,
) -> anyhow::Result<Split> {
    let mut graph = Graph::default();
    for rel in lib_files {
        graph.add_file(root, rel)?;
    }
    let reachable = graph.reachable_from(foreign);

    let mut split = Split {
        kept: Vec::new(),
        rescued: Vec::new(),
    };
    for hit in hits {
        if reachable.contains(&hit.symbol) {
            split.rescued.push(hit);
        } else {
            split.kept.push(hit);
        }
    }
    Ok(split)
}

#[derive(Default)]
struct Graph {
    /// name -> the identifiers inside each item carrying that name.
    items: BTreeMap<String, Vec<BTreeSet<String>>>,
    /// (identifiers in the header, identifiers in the whole block) of every trait impl.
    trait_impls: Vec<(BTreeSet<String>, BTreeSet<String>)>,
}

impl Graph {
    fn add_file(&mut self, root: &Path, rel: &str) -> anyhow::Result<()> {
        let source = std::fs::read_to_string(root.join(rel))?;
        let parsed = syn::parse_file(&source)
            .map_err(|e| anyhow::anyhow!("could not parse {rel} with syn ({e})"))?;
        let lines: Vec<&str> = source.lines().collect();
        let mut collector = Collector {
            lines: &lines,
            graph: self,
        };
        collector.visit_file(&parsed);
        Ok(())
    }

    fn reachable_from(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = seeds.iter().cloned().collect();
        let mut done_impls = vec![false; self.trait_impls.len()];
        loop {
            while let Some(name) = queue.pop() {
                if !reached.insert(name.clone()) {
                    continue;
                }
                for idents in self.items.get(&name).into_iter().flatten() {
                    queue.extend(idents.iter().filter(|i| !reached.contains(*i)).cloned());
                }
            }
            let mut grew = false;
            for (i, (header, body)) in self.trait_impls.iter().enumerate() {
                if !done_impls[i] && header.iter().any(|h| reached.contains(h)) {
                    done_impls[i] = true;
                    queue.extend(body.iter().filter(|b| !reached.contains(*b)).cloned());
                    grew = true;
                }
            }
            if !grew {
                return reached;
            }
        }
    }
}

struct Collector<'a> {
    lines: &'a [&'a str],
    graph: &'a mut Graph,
}

impl Collector<'_> {
    /// Identifiers on lines `first..=last` (1-based, inclusive).
    fn idents(&self, first: usize, last: usize) -> BTreeSet<String> {
        let last = last.min(self.lines.len());
        if first == 0 || first > last {
            return BTreeSet::new();
        }
        identifiers_in(&self.lines[first - 1..last].join("\n"))
    }

    fn record(&mut self, ident: &syn::Ident, whole: proc_macro2::Span) {
        let idents = self.idents(whole.start().line, whole.end().line);
        self.graph
            .items
            .entry(ident.to_string().trim_start_matches("r#").to_string())
            .or_default()
            .push(idents);
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_item_fn(self, i);
    }
    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_struct(self, i);
    }
    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_enum(self, i);
    }
    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_union(self, i);
    }
    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_trait(self, i);
    }
    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_type(self, i);
    }
    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_const(self, i);
    }
    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_static(self, i);
    }
    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_impl_item_fn(self, i);
    }
    fn visit_impl_item_const(&mut self, i: &'ast syn::ImplItemConst) {
        self.record(&i.ident, i.span());
        syn::visit::visit_impl_item_const(self, i);
    }
    fn visit_impl_item_type(&mut self, i: &'ast syn::ImplItemType) {
        self.record(&i.ident, i.span());
        syn::visit::visit_impl_item_type(self, i);
    }
    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_trait_item_fn(self, i);
    }
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_some() {
            let whole = i.span();
            let header_last = i.brace_token.span.open().start().line;
            let header = self.idents(whole.start().line, header_last);
            let body = self.idents(whole.start().line, whole.end().line);
            self.graph.trait_impls.push((header, body));
        }
        syn::visit::visit_item_impl(self, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(file: &str, line: u32, symbol: &str) -> DeadCodeHit {
        DeadCodeHit {
            file: file.into(),
            line,
            symbol: symbol.into(),
        }
    }

    fn names(hits: &[DeadCodeHit]) -> Vec<&str> {
        hits.iter().map(|h| h.symbol.as_str()).collect()
    }

    fn foreign(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// A one-file lib; returns (dir, lib_files).
    fn lib(source: &str) -> (tempfile::TempDir, BTreeSet<String>) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), source).unwrap();
        (tmp, ["src/lib.rs".to_string()].into_iter().collect())
    }

    fn run(source: &str, hits: &[(u32, &str)], seeds: &[&str]) -> Split {
        let (tmp, files) = lib(source);
        let hits = hits.iter().map(|(l, n)| hit("src/lib.rs", *l, n)).collect();
        rescue(hits, &foreign(seeds), tmp.path(), &files).unwrap()
    }

    const CHAIN: &str = "\
pub(crate) fn rescued_root() -> i32 {
    helper_b() + 1
}
pub(crate) fn helper_b() -> i32 { deep_c() }
pub(crate) fn deep_c() -> i32 { 3 }
pub(crate) fn dead_a() -> i32 { dead_b() }
pub(crate) fn dead_b() -> i32 { 4 }
pub(crate) fn unrelated() -> i32 { 5 }
";

    const CHAIN_HITS: [(u32, &str); 6] = [
        (1, "rescued_root"),
        (4, "helper_b"),
        (5, "deep_c"),
        (6, "dead_a"),
        (7, "dead_b"),
        (8, "unrelated"),
    ];

    #[test]
    fn a_candidate_a_foreign_file_names_is_rescued_and_so_is_everything_it_reaches() {
        let split = run(CHAIN, &CHAIN_HITS, &["rescued_root"]);
        assert_eq!(
            names(&split.rescued),
            vec!["rescued_root", "helper_b", "deep_c"]
        );
        assert_eq!(names(&split.kept), vec!["dead_a", "dead_b", "unrelated"]);
    }

    #[test]
    fn with_nothing_foreign_nothing_is_rescued() {
        let split = run(CHAIN, &CHAIN_HITS, &[]);
        assert!(split.rescued.is_empty());
        assert_eq!(split.kept.len(), 6);
    }

    #[test]
    fn a_mention_outside_the_reached_items_own_lines_does_not_rescue() {
        let split = run(CHAIN, &CHAIN_HITS, &["helper_b"]);
        assert_eq!(names(&split.rescued), vec!["helper_b", "deep_c"]);
    }

    #[test]
    fn a_name_shared_with_something_reachable_is_rescued_the_accepted_over_rescue() {
        let split = run(CHAIN, &CHAIN_HITS, &["unrelated"]);
        assert_eq!(names(&split.rescued), vec!["unrelated"]);
    }

    #[test]
    fn the_path_may_pass_through_an_item_that_is_not_a_candidate() {
        // The case a self-scan of CodeRipper exposed: bin -> entry (candidate) -> middle (live as far
        // as rustc says, so NOT a candidate) -> leaf (candidate). Only entry is named by the bin.
        let src = "\
pub(crate) fn entry() { middle(); }
fn middle() { leaf(); }
pub(crate) fn leaf() {}
pub(crate) fn orphan() {}
";
        let split = run(src, &[(1, "entry"), (3, "leaf"), (4, "orphan")], &["entry"]);
        assert_eq!(names(&split.rescued), vec!["entry", "leaf"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    #[test]
    fn a_trait_impl_is_live_with_its_type_so_what_its_methods_call_is_reachable() {
        // Nothing mentions `drop` by name; it runs because `Guard` is used. `cleanup` must be rescued.
        let src = "\
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        cleanup();
    }
}
pub(crate) fn cleanup() {}
pub(crate) fn orphan() {}
";
        let split = run(
            src,
            &[(1, "Guard"), (7, "cleanup"), (8, "orphan")],
            &["Guard"],
        );
        assert_eq!(names(&split.rescued), vec!["Guard", "cleanup"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    #[test]
    fn an_unreached_trait_impl_does_not_rescue_what_it_calls() {
        let src = "\
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        cleanup();
    }
}
pub(crate) fn cleanup() {}
";
        let split = run(src, &[(1, "Guard"), (7, "cleanup")], &[]);
        assert_eq!(split.kept.len(), 2);
    }

    #[test]
    fn a_dead_inherent_method_of_a_reached_type_is_still_reported() {
        // The type is used by the bin; its method `never_called` is mentioned by nobody.
        let src = "\
pub(crate) struct S;
impl S {
    pub(crate) fn used() {}
    pub(crate) fn never_called() {}
}
";
        let split = run(
            src,
            &[(1, "S"), (3, "used"), (4, "never_called")],
            &["S", "used"],
        );
        assert_eq!(names(&split.rescued), vec!["S", "used"]);
        assert_eq!(names(&split.kept), vec!["never_called"]);
    }

    #[test]
    fn a_candidate_with_no_item_of_its_own_is_rescued_only_by_a_reachable_name() {
        // dead_code also reports struct fields and enum variants; they are not items here.
        let src = "pub(crate) struct S {\n    field: i32,\n}\n";
        let hits = [(1, "S"), (2, "field")];
        assert_eq!(run(src, &hits, &[]).kept.len(), 2);
        // S's own source mentions `field`, so reaching S reaches the field
        assert_eq!(names(&run(src, &hits, &["S"]).rescued), vec!["S", "field"]);
    }

    #[test]
    fn a_multiline_items_whole_body_counts() {
        let src = "pub(crate) fn big() {\n    let a = 1;\n    let b = 2;\n    deep();\n}\npub(crate) fn deep() {}\n";
        let split = run(src, &[(1, "big"), (6, "deep")], &["big"]);
        assert_eq!(names(&split.rescued), vec!["big", "deep"]);
    }
}
````

- [ ] **Step 4: Run the tests.** Run: `cargo test --lib rescue`
  Expected: PASS, 10 tests.

- [ ] **Step 5: Prove three guards can fail.** (a) In `reachable_from`, make the `items` lookup add nothing (`for idents in std::iter::empty::<&BTreeSet<String>>()`): the chain tests must FAIL. (b) Remove the trait-impl loop (`for (i, (header, body)) in ...`): `a_trait_impl_is_live_with_its_type_so_what_its_methods_call_is_reachable` must FAIL. (c) In `visit_item_impl`, push *every* impl (drop the `if i.trait_.is_some()`): `a_dead_inherent_method_of_a_reached_type_is_still_reported` must FAIL. Revert each.

- [ ] **Step 6: Format and commit.** `cargo fmt && git add -A src && git commit -m "feat(reachability): name-based reachability from the other targets over the whole lib"`. (Not clippy-gated.)

---

### Task 4: Wire it in, replace the refusal's test, document

**Files:** Modify `src/checks/reachability/mod.rs`, `src/checks/reachability/diagnostics.rs`, `tests/reachability.rs`, `README.md`, `docs/superpowers/specs/2026-09-30-audit-host-design.md`.

**Interfaces:**
- Consumes: Tasks 1–3. `ReachabilityCheck::run`: `has_lib = src/lib.rs exists`; `Targets::LibOnly` if so, else `Targets::All`; after the sentinel check, with a lib: `lib_module_files` → `foreign_identifiers` → `rescue`. Findings are unchanged in shape (`subject` = symbol); their `detail` gains the "set aside" note when `rescued` is non-empty.

- [ ] **Step 1: Write the failing end-to-end tests first.** Apply this diff to `tests/reachability.rs`. It adds `mod common;`, **deletes**
  `a_crate_that_fails_to_compile_after_the_rewrite_errors_instead_of_reporting_clean` (it pinned the refusal this plan removes) and appends the new tests:

````diff
diff --git a/tests/reachability.rs b/tests/reachability.rs
--- a/tests/reachability.rs
+++ b/tests/reachability.rs
@@ -1,5 +1,8 @@
+mod common;
+
 use coderipper::check::{Check, CheckContext};
 use coderipper::checks::reachability::ReachabilityCheck;
+use common::{git_repo_with, MANIFEST};
 use std::process::Command;
 
 fn fixture_as_a_git_repo() -> tempfile::TempDir {
@@ -160,65 +163,178 @@ fn a_crate_wide_allow_dead_code_makes_the_check_error_not_silently_report_clean(
     );
 }
 
-#[test]
-fn a_crate_that_fails_to_compile_after_the_rewrite_errors_instead_of_reporting_clean() {
-    // Review finding (Critical), the CodeRipper-self-scan case: a package with a lib target AND a
-    // bin/tests target that imports the lib by crate name fails to compile once the lib's pub
-    // items are downgraded (E0603, "module is private") -- confirmed on CodeRipper's own repo
-    // during review. `ReachabilityCheck::run` used to return `Ok(Vec::new())` for this, which
-    // looked identical to "genuinely clean" from the caller's side. Reproduced here with a minimal
-    // lib+bin package (not the full coderipper repo, to keep this test fast and self-contained).
-    let tmp = tempfile::tempdir().unwrap();
-    std::fs::write(
-        tmp.path().join("Cargo.toml"),
-        "[package]\nname = \"libbin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
-    )
-    .unwrap();
-    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
-    std::fs::write(
-        tmp.path().join("src/lib.rs"),
-        "pub fn helper() -> i32 { 1 }\n",
-    )
-    .unwrap();
-    std::fs::write(
-        tmp.path().join("src/main.rs"),
-        "fn main() { println!(\"{}\", libbin::helper()); }\n",
-    )
-    .unwrap();
-    Command::new("git")
-        .arg("init")
-        .arg("-q")
-        .current_dir(tmp.path())
-        .status()
-        .unwrap();
-    Command::new("git")
-        .args(["add", "-A"])
-        .current_dir(tmp.path())
-        .status()
-        .unwrap();
-    Command::new("git")
-        .args([
-            "-c",
-            "user.email=t@t",
-            "-c",
-            "user.name=t",
-            "commit",
-            "-q",
-            "-m",
-            "init",
-        ])
-        .current_dir(tmp.path())
-        .status()
-        .unwrap();
+fn subjects(findings: &[coderipper::finding::Finding]) -> Vec<String> {
+    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
+    v.sort();
+    v
+}
 
-    let ctx = CheckContext {
-        project_root: tmp.path().to_path_buf(),
-        portfolio_root: tmp.path().to_path_buf(),
-    };
+fn run_reachability(repo: &tempfile::TempDir) -> anyhow::Result<Vec<coderipper::finding::Finding>> {
+    ReachabilityCheck.run(&CheckContext {
+        project_root: repo.path().to_path_buf(),
+        portfolio_root: repo.path().to_path_buf(),
+    })
+}
 
-    let result = ReachabilityCheck.run(&ctx);
+const LIB_WITH_BIN_USERS: &str = "\
+pub mod api {
+    pub fn used_by_bin() -> i32 { 1 }
+    pub fn helper_of_bin_only() -> i32 { 2 }
+    pub fn used_by_lib() -> i32 { 3 }
+    pub fn dead() -> i32 { 4 }
+    pub struct Cfg;
+    pub struct DeadTy;
+}
+pub use api::Cfg;
+pub fn entry() -> i32 { api::used_by_lib() }
+pub fn bin_root() -> i32 { api::helper_of_bin_only() }
+";
+
+const BIN_USING_THE_LIB: &str = "\
+use fixture::Cfg;
+fn main() {
+    let _c = Cfg;
+    println!(\"{} {}\", fixture::api::used_by_bin(), fixture::bin_root());
+}
+";
+
+#[test]
+fn a_lib_consumed_by_its_own_bin_is_analyzed_and_what_the_bin_uses_is_not_reported() {
+    // The case the check used to refuse (E0603 after downgrading the lib's pub items): the lib is
+    // built alone, and an item a bin names is rescued, together with everything it reaches.
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", LIB_WITH_BIN_USERS),
+        ("src/main.rs", BIN_USING_THE_LIB),
+    ]);
+    let findings = run_reachability(&repo).unwrap();
+    // used_by_bin, bin_root and Cfg are named by the bin; helper_of_bin_only is only reached from
+    // bin_root (transitive rescue). Nothing in the lib or the bin reaches the other four.
+    assert_eq!(
+        subjects(&findings),
+        vec!["DeadTy", "dead", "entry", "used_by_lib"]
+    );
+    assert!(findings.iter().all(|f| f.positive_control.is_some()));
     assert!(
-        result.is_err(),
-        "a lib+bin crate that fails to compile after the rewrite must error, not report clean"
+        findings[0].detail.contains("not reported"),
+        "the finding should say that candidates were set aside: {}",
+        findings[0].detail
+    );
+}
+
+#[test]
+fn integration_tests_examples_benches_src_bin_and_bin_only_modules_all_rescue() {
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        (
+            "src/lib.rs",
+            "pub fn from_test() {}\npub fn from_example() {}\npub fn from_bench() {}\npub fn from_bin() {}\npub fn from_bin_module() {}\npub fn nobody() {}\n",
+        ),
+        ("tests/t.rs", "#[test]\nfn t() { fixture::from_test(); }\n"),
+        ("examples/e.rs", "fn main() { fixture::from_example(); }\n"),
+        ("benches/b.rs", "fn main() { fixture::from_bench(); }\n"),
+        ("src/bin/tool.rs", "fn main() { fixture::from_bin(); }\n"),
+        ("src/main.rs", "mod util;\nfn main() { util::go(); }\n"),
+        ("src/util.rs", "pub fn go() { fixture::from_bin_module(); }\n"),
+    ]);
+    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["nobody"]);
+}
+
+#[test]
+fn a_mention_inside_the_lib_itself_does_not_rescue() {
+    // `src/inner.rs` is part of the lib, so its text is not a foreign mention: a dead item there
+    // stays reported even though the bin mentions an unrelated name.
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", "mod inner;\npub use inner::kept_alive;\n"),
+        (
+            "src/inner.rs",
+            "pub fn kept_alive() {}\npub fn dead_in_inner() {}\n",
+        ),
+        ("src/main.rs", "fn main() { fixture::kept_alive(); }\n"),
+    ]);
+    assert_eq!(
+        subjects(&run_reachability(&repo).unwrap()),
+        vec!["dead_in_inner"]
     );
 }
+
+#[test]
+fn a_name_shared_with_something_the_bin_uses_is_rescued_that_is_the_accepted_over_rescue() {
+    // `Tool::new` is dead in the lib; the bin calls a DIFFERENT `new`. By design the name wins:
+    // a missed finding, never a wrongly reported live item.
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", "pub struct Tool;\nimpl Tool { pub fn new() -> Tool { Tool } }\npub struct Used;\nimpl Used { pub fn new() -> Used { Used } }\n"),
+        ("src/main.rs", "fn main() { let _u = fixture::Used::new(); }\n"),
+    ]);
+    // Without the name match, the dead `Tool::new` would be the one finding.
+    let found = subjects(&run_reachability(&repo).unwrap());
+    assert!(found.is_empty(), "{found:?}");
+}
+
+#[test]
+fn deny_warnings_on_a_lib_does_not_stop_the_analysis() {
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        (
+            "src/lib.rs",
+            "#![deny(warnings)]\nuse std::collections::HashMap;\npub fn dead() {}\n",
+        ),
+        ("src/main.rs", "fn main() {}\n"),
+    ]);
+    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["dead"]);
+}
+
+#[test]
+fn a_crate_that_really_does_not_compile_still_errors() {
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", "pub fn f() { let x: i32 = \"no\"; }\n"),
+        ("src/main.rs", "fn main() {}\n"),
+    ]);
+    assert!(run_reachability(&repo).is_err());
+}
+
+#[test]
+fn a_path_from_the_bin_through_a_trait_impl_keeps_everything_it_reaches_alive() {
+    // The shape a self-scan of CodeRipper itself exposed: the bin names `run_all`, which reaches
+    // `registry`, whose trait impl's method calls `helper`. The trait impl is not a dead-code
+    // candidate, so a rescue that only follows candidates loses the path and reports live code.
+    let lib = "\
+pub trait Check { fn go(&self) -> i32; }
+pub struct A;
+impl Check for A { fn go(&self) -> i32 { helper() } }
+pub fn helper() -> i32 { 1 }
+pub fn registry() -> Vec<Box<dyn Check>> { vec![Box::new(A)] }
+pub fn run_all() -> i32 { registry().iter().map(|c| c.go()).sum() }
+pub fn orphan() -> i32 { 2 }
+";
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", lib),
+        (
+            "src/main.rs",
+            "fn main() { println!(\"{}\", fixture::run_all()); }\n",
+        ),
+    ]);
+    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["orphan"]);
+}
+
+#[test]
+fn a_constant_used_only_through_a_format_string_capture_is_alive() {
+    let lib = "\
+pub const GREETING: &str = \"hi\";
+pub const NEVER: &str = \"unused\";
+pub fn run_all() -> String { format!(\"{GREETING}, world\") }
+";
+    let repo = git_repo_with(&[
+        ("Cargo.toml", MANIFEST),
+        ("src/lib.rs", lib),
+        (
+            "src/main.rs",
+            "fn main() { println!(\"{}\", fixture::run_all()); }\n",
+        ),
+    ]);
+    assert_eq!(subjects(&run_reachability(&repo).unwrap()), vec!["NEVER"]);
+}
````

  Run: `cargo test --test reachability`
  Expected: FAIL — the lib+bin test, the `src/bin`/examples/benches test, `a_mention_inside_the_lib_itself_does_not_rescue` and the over-rescue test fail
  (the check still builds `--all-targets` and errors with E0603); `deny_warnings_on_a_lib_does_not_stop_the_analysis` and `a_crate_that_really_does_not_compile_still_errors`
  already pass — they are guards that must keep passing. (`a_path_from_the_bin_through_a_trait_impl...` and `a_constant_used_only_through_a_format_string_capture_is_alive` are in the same diff; they fail here too.)

- [ ] **Step 2: Wire it in.** The `mod.rs` change selects the targets and applies the rescue; `diagnostics.rs` loses the `collect_dead_code` wrapper (now unused) and its tests call `collect_dead_code_in(.., Targets::All)`; the docs say what changed:

````diff
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -26,6 +26,13 @@ never used) and `unused-return-values` (a function whose
 return value every caller discards). See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
 `docs/superpowers/plans/` for what's actually being built and in what order.
 
+## Reachability on a package with a library
+
+`reachability` builds only the library when there is one, then does not report an item that a bin, test,
+example or bench reaches by name (and everything that item reaches). It can therefore miss a dead item that
+shares a name with something live, but it does not report live code as dead. `pub` items inside the bins
+themselves are not analyzed when a library exists.
+
 ## Suppressing a finding
 
 Some findings are deliberate (public API built ahead of its consumer, a value discarded on purpose).
diff --git a/docs/superpowers/specs/2026-09-30-audit-host-design.md b/docs/superpowers/specs/2026-09-30-audit-host-design.md
--- a/docs/superpowers/specs/2026-09-30-audit-host-design.md
+++ b/docs/superpowers/specs/2026-09-30-audit-host-design.md
@@ -100,6 +100,17 @@ trick, not new technology, just not packaged as a repeatable check here yet. Pyt
 TypeScript (`ts-prune` or similar) already have adequate tools; this check just wires their output into the
 shared `Finding` schema.
 
+**Packages with a library (Rust), implemented 2026-10-02** (`docs/superpowers/plans/2026-10-02-reachability-lib-plus-bin.md`).
+Downgrading a lib's `pub` items breaks every other target that imports it by crate name (E0603), so the check
+used to refuse such packages, CodeRipper itself included. Now only the library is built, and a candidate is
+*rescued* (not reported) when a file the lib does not compile (a bin, an integration test, an example, a bench,
+or a module only a bin declares) reaches it by name: reachability over identifiers across the whole lib, with
+every `impl Trait for Type` block live as soon as its trait or type is reachable. It can over-rescue (a dead `new`
+hidden because something reachable mentions another `new`), never report a live item as dead; the cost is
+missed findings on common names. Not covered: `pub` items inside bin targets when a lib exists, and a custom
+`[lib] path`. Acceptance probe: the check now runs on CodeRipper itself and reports nothing, and three planted
+dead items are found.
+
 **Sub-pass B, `Portfolio` scope.** Does every project's declared public API surface actually get referenced
 by another portfolio repo meant to consume it?
 
diff --git a/src/checks/reachability/diagnostics.rs b/src/checks/reachability/diagnostics.rs
--- a/src/checks/reachability/diagnostics.rs
+++ b/src/checks/reachability/diagnostics.rs
@@ -20,10 +20,6 @@ pub enum Targets {
     LibOnly,
 }
 
-pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult> {
-    collect_dead_code_in(worktree_root, Targets::All)
-}
-
 pub fn collect_dead_code_in(
     worktree_root: &Path,
     targets: Targets,
@@ -80,7 +76,7 @@ fn extract_symbol_names(message: &str) -> Vec<String> {
 
 #[cfg(test)]
 mod tests {
-    use super::{collect_dead_code, CollectResult};
+    use super::{collect_dead_code_in, CollectResult, Targets};
     use std::path::Path;
 
     fn write_fixture(dir: &Path, lib_rs: &str) {
@@ -101,7 +97,7 @@ mod tests {
         let CollectResult {
             hits,
             build_failed_for_other_reasons,
-        } = collect_dead_code(tmp.path()).unwrap();
+        } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
 
         assert!(!build_failed_for_other_reasons);
         assert_eq!(hits.len(), 1);
@@ -128,7 +124,7 @@ mod tests {
             "pub(crate) fn used() -> i32 { 1 }\npub fn caller() -> i32 { used() }\n",
         );
 
-        let CollectResult { hits, .. } = collect_dead_code(tmp.path()).unwrap();
+        let CollectResult { hits, .. } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
         assert!(
             hits.is_empty(),
             "the pipeline must be able to see a real caller when one exists"
@@ -147,7 +143,7 @@ mod tests {
             "pub(crate) struct Foo;\nimpl Foo {\n    fn a(&self) {}\n    fn b(&self) {}\n    fn c(&self) {}\n}\n",
         );
 
-        let CollectResult { hits, .. } = collect_dead_code(tmp.path()).unwrap();
+        let CollectResult { hits, .. } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
         let mut symbols: Vec<&str> = hits.iter().map(|h| h.symbol.as_str()).collect();
         symbols.sort();
         assert_eq!(symbols, vec!["Foo", "a", "b", "c"]);
@@ -164,7 +160,7 @@ mod tests {
         let CollectResult {
             hits,
             build_failed_for_other_reasons,
-        } = collect_dead_code(tmp.path()).unwrap();
+        } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
 
         assert!(!build_failed_for_other_reasons);
         assert_eq!(hits.len(), 1);
@@ -185,7 +181,7 @@ mod tests {
         let CollectResult {
             hits,
             build_failed_for_other_reasons,
-        } = collect_dead_code(tmp.path()).unwrap();
+        } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
 
         assert!(!build_failed_for_other_reasons);
         assert_eq!(hits.len(), 1);
@@ -215,7 +211,7 @@ mod tests {
         let CollectResult {
             hits,
             build_failed_for_other_reasons,
-        } = collect_dead_code(tmp.path()).unwrap();
+        } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
 
         assert!(build_failed_for_other_reasons);
         assert!(hits.is_empty());
@@ -229,7 +225,7 @@ mod tests {
         let CollectResult {
             hits,
             build_failed_for_other_reasons,
-        } = collect_dead_code(tmp.path()).unwrap();
+        } = collect_dead_code_in(tmp.path(), Targets::All).unwrap();
 
         assert!(build_failed_for_other_reasons);
         assert!(
diff --git a/src/checks/reachability/mod.rs b/src/checks/reachability/mod.rs
--- a/src/checks/reachability/mod.rs
+++ b/src/checks/reachability/mod.rs
@@ -1,12 +1,17 @@
 //! Reachability check (project scope): does every function in a Rust crate get called from
 //! somewhere, including `pub` items that `rustc`'s own `dead_code` lint deliberately exempts.
 //!
-//! **Known limitations, found and confirmed during implementation and review (not fixed here):**
-//! - **A package with more than one compilation target** (a `lib` + a `bin`, or a `lib` +
-//!   integration tests) where the other target imports the lib by crate name cannot be analyzed at
-//!   all: downgrading the lib's `pub` items breaks that import with `E0603`. **True of CodeRipper's
-//!   own repo.** [`inject_sentinel`] + the build-error check below detect this and return `Err`
-//!   rather than silently reporting zero findings.
+//! **Packages with a library:** only the library is built (the `pub` downgrade would break every
+//! other target that imports it by crate name). A lib item that a bin, an integration test, an
+//! example or a bench names is therefore NOT reported, and neither is anything such an item reaches:
+//! matching is by bare identifier (`rescue`), so it can over-rescue (a dead `new` is hidden when a bin
+//! calls some other `new`) but never reports a live item as dead.
+//!
+//! **Known limitations, found and confirmed during implementation and review:**
+//! - **`pub` items inside bin targets are not analyzed** when the package also has a library: rustc
+//!   exempts them unless downgraded, and downgrading a bin against an unmodified lib is a separate
+//!   pass. A package with no library is analyzed as before (all targets).
+//! - **Custom `[lib] path`** is not followed: only `src/lib.rs` counts as the library.
 //! - **A function called only from a `#[test]`** is not rescued from `dead_code` by rustc (confirmed
 //!   empirically, many independent fresh builds) -- "a test counts as reachability" (the design
 //!   doc's stated intent) is not delivered for this case.
@@ -27,7 +32,9 @@ mod sentinel;
 use crate::check::{Check, CheckContext, Network, Scope};
 use crate::finding::{Confidence, Finding, Location, Severity};
 use crate::worktree::RewrittenWorktree;
-use diagnostics::collect_dead_code;
+use diagnostics::{collect_dead_code_in, Targets};
+use foreign::{foreign_identifiers, lib_module_files};
+use rescue::rescue;
 use rewriter::rewrite_pub_to_pub_crate;
 use sentinel::{inject_sentinel, SENTINEL_SYMBOL};
 
@@ -51,31 +58,23 @@ impl Check for ReachabilityCheck {
             Ok(rewrite_pub_to_pub_crate(source))
         })?;
         let sentinel_file = inject_sentinel(&wt.root)?;
-        let result = collect_dead_code(&wt.root)?;
+        // With a library, build ONLY the library: downgrading its `pub` items breaks every other target
+        // that imports it by crate name (E0603), so bins, tests, examples and benches are not compiled.
+        // What they use is accounted for by `rescue` below instead.
+        let has_lib = wt.root.join("src/lib.rs").is_file();
+        let targets = if has_lib {
+            Targets::LibOnly
+        } else {
+            Targets::All
+        };
+        let result = collect_dead_code_in(&wt.root, targets)?;
 
-        // The sentinel is a REAL per-run positive control, not a fixed narrative string: it's a
-        // guaranteed-dead function injected into THIS run's own rewritten tree. If it doesn't come
-        // back as a hit, this run's result can't be trusted -- whether because the build failed
-        // outright (lib+bin crates importing the lib by name, confirmed on CodeRipper's own repo
-        // during review: E0603 "module is private"), or for any other reason dead_code detection
-        // didn't actually fire (e.g. a crate-wide #[allow(dead_code)]). Either way, reporting empty
-        // findings here would be indistinguishable from "genuinely clean", which is exactly the
-        // silent-false-negative this check exists to prevent elsewhere -- so it must not commit it
-        // itself. Known, accepted gap this doesn't fix (see the plan/design docs): a package with a
-        // separate lib target consumed by its own bin/tests will always trip this and report an
-        // error rather than partial findings.
-        // Check the genuine-error signal FIRST, unconditionally -- a real compile error anywhere
-        // in the build (e.g. a package with a lib target consumed by its own bin/integration-tests
-        // by crate name: downgrading the lib's pub items breaks E0603 in the OTHER target, even
-        // though the LIB half compiles fine on its own and can still show its own sentinel as
-        // confirmed) must never look like a clean result, regardless of what the sentinel found.
+        // A real compile error anywhere in the build must never look like a clean result, whatever
+        // the sentinel found.
         anyhow::ensure!(
             !result.build_failed_for_other_reasons,
             "this crate's build reported real compiler error(s) unrelated to dead_code after the \
-             rewrite -- most commonly a package with a lib target consumed by its own bin or \
-             integration tests by crate name (downgrading the lib's pub items to pub(crate) breaks \
-             E0603 'module is private' in the other target). Known gap, not fixed here -- see \
-             docs/superpowers/specs/2026-09-30-audit-host-design.md."
+             rewrite, so no result can be trusted"
         );
 
         // The sentinel is a REAL per-run positive control, not a fixed narrative string: it's a
@@ -102,10 +101,32 @@ impl Check for ReachabilityCheck {
             .map(|n| n.to_string_lossy().to_string())
             .unwrap_or_default();
 
-        let findings = result
+        let candidates: Vec<_> = result
             .hits
             .into_iter()
             .filter(|hit| hit.symbol != SENTINEL_SYMBOL) // the sentinel itself is not a real finding
+            .collect();
+        // Items a target the lib build never compiled (bin, test, example, bench) names are live as
+        // far as this check can tell; set them aside, and everything they reach.
+        let (candidates, set_aside) = if has_lib {
+            let lib_files = lib_module_files(&wt.root)?;
+            let foreign = foreign_identifiers(&wt.root, &lib_files)?;
+            let split = rescue(candidates, &foreign, &wt.root, &lib_files)?;
+            (split.kept, split.rescued.len())
+        } else {
+            (candidates, 0)
+        };
+        let set_aside_note = if set_aside == 0 {
+            String::new()
+        } else {
+            format!(
+                " {set_aside} other candidate(s) in this crate were not reported because a file the \
+                 library does not compile (a bin, test, example or bench) mentions their name."
+            )
+        };
+
+        let findings = candidates
+            .into_iter()
             .map(|hit| Finding {
                 check_id: "reachability".into(),
                 severity: Severity::Medium,
@@ -120,7 +141,7 @@ impl Check for ReachabilityCheck {
                 detail: format!(
                     "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
                      `pub(crate)` in a throwaway worktree, so the lint's normal `pub`-exemption doesn't \
-                     hide it. File: {}, line {}.",
+                     hide it. File: {}, line {}.{set_aside_note}",
                     hit.file, hit.line
                 ),
                 positive_control: Some(format!(
````

- [ ] **Step 3: Run everything.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 103 lib, 8 allowlist, 8 cli, 11 reachability, 11 unused-parameters, 13 unused-return-values.

- [ ] **Step 4: Prove three guards can fail.** (a) Pass an empty set instead of `&foreign` to `rescue(...)`: six `reachability` tests must FAIL (lib+bin, `src/bin`/examples/benches, `a_mention_inside_the_lib_itself_does_not_rescue`, the over-rescue, trait-impl-path and format-string tests). (b) Replace `let has_lib = wt.root.join("src/lib.rs").is_file();` with `let has_lib = false;`: the same six FAIL (E0603 returns). (c) In `rescue.rs`, delete the `self.record(...)` line in `visit_impl_item_fn` AND change `if i.trait_.is_some() {` in `visit_item_impl` to `if false {` (the two mechanisms are redundant for this shape, so both must go): `a_path_from_the_bin_through_a_trait_impl_keeps_everything_it_reaches_alive` must FAIL with `helper` reported. Revert each.

- [ ] **Step 5: Lint.** Run: `cargo clippy --all-targets -- -D warnings` (no output; the `dead_code` from Task 1 is gone — the `collect_dead_code` wrapper was removed in this task).

- [ ] **Step 6: Acceptance probe — the check scans CodeRipper itself.** `git clone -c core.autocrlf=false <this branch> /tmp/self`, then
  `<built-binary> check reachability --project /tmp/self`.
  Expected: `coderipper: no issues found` (measured at plan time on this branch: refused before; first draft 77 false positives; second draft 3; final 0; ~16 s).
  Then append to the scratch clone's `src/lib.rs` (and commit there only):
  `pub fn zzz_dead(a: i32) -> i32 { a }`, `pub struct ZzzDead;`, `impl ZzzDead { pub fn zzz_method(&self) {} }`, and re-run.
  Expected: exactly three findings — `zzz_dead`, `ZzzDead`, `zzz_method`.

- [ ] **Step 7: Commit, push, report.** `git add -A && git commit -m "feat(reachability): analyze packages with a library -- lib-only build plus rescue; docs"`, push the branch, and report to the PM
  (who opens and merges the PR; note that it is stacked on #9 until that merges). Read the PR's checks and **unresolved review threads**
  (GraphQL `reviewThreads{isResolved}`) before calling it READY.
