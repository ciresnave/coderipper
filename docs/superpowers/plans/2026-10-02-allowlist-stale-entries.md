# Allowlist: stale-this-run suppression detection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a project's `.coderipper.toml` suppresses a finding, tell the user — as an ordinary
`Info` finding — about every suppression that matched nothing in *this* run, so a suppression whose cause
has been fixed does not linger forever.

**Architecture:** Today each check loads the allowlist and filters its own output, so nobody can see an
entry that matched nothing. Move suppression into the host (`run_checks`): checks return their RAW
findings; the host applies the allowlist, remembers which entries matched, and afterwards emits one `Info`
finding per entry that matched nothing in a check that ran to completion. A finding needs a fingerprint for
an entry to name it, so `Finding` gains `subject` (the symbol it is about).

**Tech Stack:** Rust 2021, no new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-30-audit-host-design.md` §4 ("Allowlist mechanism, generalized
across checks"). **Scope is narrower than §4's text, by the PM's explicit call (2026-10-02):** only the
universal trigger, only its **stale-this-run** form. Out of scope and documented as future extensions, not
built: "hasn't matched in N runs" (needs a run-history store that doesn't exist — revisit if one is built
for another reason) and the `until_fixed` link to a different named finding. That settles §4's "one axis or
two" open question for now as one axis. The surface is an `Info` finding of the existing `Finding` type — no
parallel reporting channel.

## Design decisions the spec leaves open (rulings, with what each costs if wrong)

1. **Fingerprint = (`check_id`, `location.file`, `subject`).** §4 says entries are "keyed by `check_id` +
   a fingerprint of the specific finding" but `Finding` had no symbol field, so the host could not match
   anything. `subject: Option<String>` is added (`#[serde(default)]`, so old JSON reports still load). A
   finding with no subject can never be allowlisted. *Cost if wrong:* a check with no natural symbol needs
   a different key later; `subject` can carry any stable string.
2. **Checks return raw findings; the host suppresses.** This changes the contract of `Check::run` for every
   future check (documented on the trait in Task 3) and removes the allowlist code from both existing checks.
   Behaviour for users is unchanged except item 4. *Cost if wrong:* a future check that wants its own
   suppression semantics has to be special-cased.
3. **An entry is judged stale only if its check ran to completion in this run** (returned `Ok` and every
   finding validated). A check that errored, was filtered out by `--check`, or belongs to a tier that did not
   run says nothing about whether its entries still match. *Cost if wrong:* a stale entry for a check that
   keeps failing is never reported — but that run already exits non-zero.
4. **A malformed `.coderipper.toml` is now ONE `allowlist: ...` error and the findings are still returned,
   unsuppressed.** Before, every check errored and returned nothing. *Cost if wrong:* a user with a broken
   file sees findings they meant to suppress, next to the error that explains why.
5. **An entry naming an unregistered check is reported regardless of which checks ran** — it can never
   match anything, whatever the run — as a separate `Info` finding listing the registered ids (a typo
   catcher).
6. **Entry paths tolerate backslashes** (`src\api.rs` is read as `src/api.rs`); findings always carry
   forward-slash paths. Found by thinking about Windows authors, then pinned by a test that first failed for
   the right reason. In TOML a backslash needs a literal string (`'src\api.rs'`) or doubling — the test uses
   the literal form.
7. **Exit status is unchanged: `Info` findings never fail a run** (only check errors do), as for every
   finding today.

## Global Constraints

- Rust edition 2021; CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --all-targets`, `cargo test --no-fail-fast` on **ubuntu, windows and macos**. The tip of the
  branch must pass all four; intermediate commits are not clippy-gated (Task 2 adds a module that is not
  wired in until Task 3, so it is `dead_code` for one commit).
- **A test must never point a check at the shared checkout** (`git worktree add` writes into the repo's
  `.git`). Use throwaway fixtures: `tests/common/mod.rs::git_repo_with` exists for this.
- A finding that claims an absence ("suppressed nothing", "not registered") must carry a `positive_control`
  or the host refuses it (`Finding::validate`). Every finding this plan adds has one.
- Do **not** bump any version in `Cargo.toml`: the portfolio PM allocates it at gate time.
- Base the branch on a **fresh** `origin/main` (`git fetch origin`); work in your own clone or worktree,
  never `checkout` in a shared tree. Clone with `-c core.autocrlf=false` on this Windows box, or every
  file shows as modified and `git add -A` will commit CRLF noise.
- If you write Rust containing a backslash literal via a shell heredoc here, the backslash can be
  silently dropped or doubled — create such files with the editor/Write tool.
- Commit trailers (this lane): end each commit message with
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_01BTQZ9Prw7JEaN1ryVr3iaG`. Do not run `gh auth switch`
  (the lane account cannot open or merge PRs here; the PM does that).

## Review Focus

Failure modes the spec is silent on that are most likely to bite someone using this. Each has a named test.

1. **An entry for a check that failed, or was not run, must NOT be called stale.** A broken build, or
   `--check other`, would otherwise flag every entry for the check that produced no information.
   → `an_entry_for_a_check_that_failed_to_run_is_not_judged`, `an_entry_for_a_registered_check_that_did_not_run_is_not_judged` (Task 3), `an_entry_for_a_check_that_did_not_complete_is_not_judged` (Task 2).
2. **Exact identity, not containment.** Same symbol in a different file must neither suppress nor count as
   matched. → `a_finding_is_suppressed_only_on_an_exact_check_file_and_subject_match` (Task 2), `a_matching_symbol_in_a_different_file_does_not_suppress_and_the_entry_is_stale` (Task 3).
3. **A typo in `check = "..."`** silently suppresses nothing forever; it must surface even when only one other
   check was asked for. → `an_unregistered_check_is_flagged_whether_or_not_anything_ran` (Task 2), `an_entry_naming_an_unknown_check_is_reported_even_when_that_check_was_not_asked_for` (Task 3).
4. **Windows-style separators in an entry.** → `an_entry_written_with_windows_separators_still_matches` (Task 2).
5. **A malformed allowlist must not swallow the findings** (and must not be silent). → `a_malformed_allowlist_is_an_error_but_does_not_swallow_the_findings` (Task 3).

**Known gaps accepted for v1 (documented, not tested):** findings with no `subject` cannot be allowlisted; a
check that returns zero raw findings makes every one of its entries stale (correct, and the stale finding's
positive control says "returned 0 raw finding(s)"); two identical entries both count as matched; entries are
not judged across a `--check` filter; "N runs" and `until_fixed` are not built.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/finding.rs` | `Finding.subject` |
| `src/allowlist.rs` | Load `.coderipper.toml`; expose entries (no matching logic any more) |
| `src/suppression.rs` (new) | Apply entries to raw findings; produce the stale / unknown-check `Info` findings |
| `src/lib.rs` | `run_checks` loads the allowlist once, applies suppression, appends stale findings |
| `src/checks/{reachability,unused_return_values}/mod.rs` | Stop filtering; set `subject` |
| `tests/allowlist.rs` (new) | Host-level behaviour |

Baseline before starting (`origin/main` at `96a30e6`): `cargo test` shows 60 lib unit tests, 5 in `tests/cli.rs`,
4 in `tests/reachability.rs`, 14 in `tests/unused_return_values.rs`, all passing.

---

### Task 1: `Finding.subject`

**Files:** Modify `src/finding.rs`, `src/checks/reachability/mod.rs`, `src/checks/unused_return_values/mod.rs`, `tests/reachability.rs`, `tests/unused_return_values.rs`.

**Interfaces:**
- Produces: `Finding.subject: Option<String>` (`#[serde(default)]`). Reachability sets it to the dead symbol's name; unused-return-values to the function's name.

- [ ] **Step 1: Write the failing tests.** Add this test to `src/finding.rs`'s `mod tests`, and the two assertions in the diff below (marked in the `tests/` hunks):

```rust
    #[test]
    fn a_finding_written_before_subject_existed_still_deserializes() {
        // Old JSON reports have no `subject`; the field must default to None, not fail.
        let json = r#"{"check_id":"reachability","severity":"high","confidence":"medium","project":"p","location":null,"summary":"s","detail":"d","positive_control":null}"#;
        let f: Finding = serde_json::from_str(json).expect("deserialize");
        assert_eq!(f.subject, None);
    }
```

- [ ] **Step 2: Run to see it fail.** Run: `cargo test finding`
  Expected: FAIL to compile — `no field subject on type Finding`.

- [ ] **Step 3: Implement.** Apply this diff (it contains the test additions from Step 1 as well, for reference):

````diff
diff --git a/src/checks/reachability/mod.rs b/src/checks/reachability/mod.rs
--- a/src/checks/reachability/mod.rs
+++ b/src/checks/reachability/mod.rs
@@ -116,6 +116,7 @@ impl Check for ReachabilityCheck {
                     file: hit.file.clone(),
                     line: Some(hit.line),
                 }),
+                subject: Some(hit.symbol.clone()),
                 summary: format!("`{}` has zero callers found within this crate", hit.symbol),
                 detail: format!(
                     "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
diff --git a/src/checks/unused_return_values/mod.rs b/src/checks/unused_return_values/mod.rs
--- a/src/checks/unused_return_values/mod.rs
+++ b/src/checks/unused_return_values/mod.rs
@@ -136,6 +136,7 @@ impl Check for UnusedReturnValuesCheck {
                         file: u.tag.file.clone(),
                         line: Some(u.tag.line),
                     }),
+                    subject: Some(u.tag.name.clone()),
                     summary,
                     detail: format!(
                         "Counted by tagging the function with `#[must_use]` and `#[deprecated]` in a \
diff --git a/src/finding.rs b/src/finding.rs
--- a/src/finding.rs
+++ b/src/finding.rs
@@ -44,6 +44,11 @@ pub struct Finding {
     pub confidence: Confidence,
     pub project: String,
     pub location: Option<Location>,
+    /// The symbol the finding is about (a function name, ...), when it is about one. Together with
+    /// `check_id` and `location.file` it is the finding's fingerprint, which is what an allowlist
+    /// entry (`.coderipper.toml`) names. `None` for findings that aren't about a single symbol.
+    #[serde(default)]
+    pub subject: Option<String>,
     pub summary: String,
     pub detail: String,
     /// Required whenever the finding's core claim is an absence ("zero callers", "no protection
@@ -100,6 +105,7 @@ mod tests {
                 file: "src/model_fuel/policies.rs".into(),
                 line: Some(626),
             }),
+            subject: Some("splice_prefix".into()),
             summary: summary.into(),
             detail: "detail text".into(),
             positive_control: positive_control.map(str::to_string),
@@ -115,6 +121,14 @@ mod tests {
         assert_eq!(back.summary, f.summary);
     }
 
+    #[test]
+    fn a_finding_written_before_subject_existed_still_deserializes() {
+        // Old JSON reports have no `subject`; the field must default to None, not fail.
+        let json = r#"{"check_id":"reachability","severity":"high","confidence":"medium","project":"p","location":null,"summary":"s","detail":"d","positive_control":null}"#;
+        let f: Finding = serde_json::from_str(json).expect("deserialize");
+        assert_eq!(f.subject, None);
+    }
+
     #[test]
     fn an_absence_claim_without_a_positive_control_is_rejected() {
         let f = sample("zero callers found for splice_prefix", None);
diff --git a/tests/reachability.rs b/tests/reachability.rs
--- a/tests/reachability.rs
+++ b/tests/reachability.rs
@@ -67,6 +67,7 @@ fn the_reachability_check_finds_exactly_the_dead_function() {
     );
     assert!(findings[0].summary.contains("`dead_function`"));
     assert!(findings[0].positive_control.is_some());
+    assert_eq!(findings[0].subject.as_deref(), Some("dead_function"));
     // caller/used_function must NOT appear as the IDENTIFIED symbol -- they're reachable via
     // main(). Match the backtick-quoted symbol name, not a bare substring: the summary template's
     // own prose ("has zero callers found") contains "caller" as a substring of "callers", which
diff --git a/tests/unused_return_values.rs b/tests/unused_return_values.rs
--- a/tests/unused_return_values.rs
+++ b/tests/unused_return_values.rs
@@ -75,6 +75,12 @@ fn it_separates_always_ignored_from_sometimes_ignored_on_a_lib_plus_bin_package(
         sometimes.summary
     );
     assert!(findings.iter().all(|f| f.positive_control.is_some()));
+    let mut subjects: Vec<_> = findings
+        .iter()
+        .map(|f| f.subject.clone().unwrap())
+        .collect();
+    subjects.sort();
+    assert_eq!(subjects, vec!["always_ignored", "sometimes_ignored"]);
     assert!(findings.iter().all(|f| f.clone().validate().is_ok()));
 }
 
````

- [ ] **Step 4: Run the whole suite.** Run: `cargo fmt && cargo test`
  Expected: PASS — 61 lib tests (60 + the new `finding` test); counts otherwise unchanged: 5 cli, 4 reachability, 14 unused-return-values. The two new `subject` assertions in the integration tests pass only because Step 3 set `subject` in both checks (before Step 3 they would not compile).

- [ ] **Step 5: Commit** — `git add -A src tests && git commit -m "feat: Finding.subject, the symbol a finding is about"`.

---

### Task 2: The suppression module (pure, not yet wired in)

**Files:** Create `src/suppression.rs`; modify `src/allowlist.rs`, `src/lib.rs` (one line).

**Interfaces:**
- Consumes: `Finding` with `subject` (Task 1).
- Produces: `crate::allowlist::{Allowlist::empty(), Allowlist::entries() -> &[AllowEntry]}` and `AllowEntry { check, file, symbol, reason }` (all `pub`, type `pub(crate)`); `Allowlist::is_allowed` is **removed**.
  `crate::suppression::Suppression<'a>` with `new(&'a Allowlist)`, `mark_completed(&mut self, check_id: &str, raw_count: usize)`, `apply(&mut self, raw: Vec<Finding>) -> Vec<Finding>`, `stale_findings(&self, registered: &[&str], project: &str) -> Vec<Finding>`; and `ALLOWLIST_CHECK_ID = "allowlist"`.
  `stale_findings` emits `Severity::Info`, `Confidence::High`, `check_id = "allowlist"`, `location = .coderipper.toml` (no line), `subject` = the entry's symbol, with a `positive_control`.

- [ ] **Step 1: Write the failing tests first.** Create `src/suppression.rs` containing ONLY the `#[cfg(test)] mod tests { ... }` block from the full file in Step 3, and declare it in `src/lib.rs` (add `pub(crate) mod suppression;` after `pub mod finding;`).
  Run: `cargo test --lib suppression`
  Expected: FAIL to compile — `cannot find type Allowlist / Finding / Location in this scope` (the module under test does not exist yet).

- [ ] **Step 2: Change `src/allowlist.rs`.** `reason` stops being validation-only (it is now shown in the stale finding), so its `#[allow(dead_code)]` goes; matching moves to `suppression`; the tests keep their meaning through a local helper:

````diff
diff --git a/src/allowlist.rs b/src/allowlist.rs
--- a/src/allowlist.rs
+++ b/src/allowlist.rs
@@ -8,17 +8,13 @@ struct AllowlistFile {
 }
 
 #[derive(Debug, Deserialize)]
-struct AllowEntry {
-    check: String,
-    file: String,
-    symbol: String,
-    // Required so a missing field fails TOML deserialization -- that's the enforcement mechanism,
-    // not programmatic use. rustc's dead_code lint deliberately does not count a derived Debug
-    // impl's field read as real usage (confirmed via its own diagnostic note), so this is an
-    // honest, compiler-suggested suppression for a genuinely validation-only field, not a
-    // workaround for a real bug. Debug is still kept for future diagnostic output.
-    #[allow(dead_code)]
-    reason: String,
+pub(crate) struct AllowEntry {
+    pub check: String,
+    pub file: String,
+    pub symbol: String,
+    /// Required: a missing field fails TOML deserialization, which is what enforces "every
+    /// suppression says why". Shown in the finding raised when the entry goes stale.
+    pub reason: String,
 }
 
 pub struct Allowlist {
@@ -26,6 +22,12 @@ pub struct Allowlist {
 }
 
 impl Allowlist {
+    pub fn empty() -> Self {
+        Self {
+            entries: Vec::new(),
+        }
+    }
+
     pub fn load(project_root: &Path) -> anyhow::Result<Self> {
         let path = project_root.join(".coderipper.toml");
         if !path.exists() {
@@ -40,10 +42,8 @@ impl Allowlist {
         })
     }
 
-    pub fn is_allowed(&self, check_id: &str, file: &str, symbol: &str) -> bool {
-        self.entries
-            .iter()
-            .any(|e| e.check == check_id && e.file == file && e.symbol == symbol)
+    pub fn entries(&self) -> &[AllowEntry] {
+        &self.entries
     }
 }
 
@@ -51,11 +51,17 @@ impl Allowlist {
 mod tests {
     use super::Allowlist;
 
+    fn allowed(al: &Allowlist, check: &str, file: &str, symbol: &str) -> bool {
+        al.entries()
+            .iter()
+            .any(|e| e.check == check && e.file == file && e.symbol == symbol)
+    }
+
     #[test]
     fn missing_allowlist_file_allows_nothing_not_an_error() {
         let tmp = tempfile::tempdir().unwrap();
         let al = Allowlist::load(tmp.path()).unwrap();
-        assert!(!al.is_allowed("reachability", "src/lib.rs", "anything"));
+        assert!(!allowed(&al, "reachability", "src/lib.rs", "anything"));
     }
 
     #[test]
@@ -74,10 +80,20 @@ reason = "published crate API, consumed outside this portfolio"
         .unwrap();
 
         let al = Allowlist::load(tmp.path()).unwrap();
-        assert!(al.is_allowed("reachability", "src/api.rs", "public_entry_point"));
+        assert!(allowed(
+            &al,
+            "reachability",
+            "src/api.rs",
+            "public_entry_point"
+        ));
         // Review Focus: must match by identity, not loose path containment.
-        assert!(!al.is_allowed("reachability", "src/api.rs", "some_other_fn"));
-        assert!(!al.is_allowed("reachability", "src/api_v2.rs", "public_entry_point"));
+        assert!(!allowed(&al, "reachability", "src/api.rs", "some_other_fn"));
+        assert!(!allowed(
+            &al,
+            "reachability",
+            "src/api_v2.rs",
+            "public_entry_point"
+        ));
     }
 
     #[test]
````

- [ ] **Step 3: Create the module — replace the tests-only file with the full file:**

````rust
// src/suppression.rs
//! Applies the project's allowlist to a run's raw findings, and notices entries that no longer
//! suppress anything (design doc §4, "self-dissolving" suppression).
//!
//! Done by the host, not by each check, because only the host sees every check's RAW
//! (pre-suppression) findings next to the entries. A check returns what it found; this decides what
//! is suppressed and which entries have outlived their cause.
//!
//! Scope, deliberately: an entry is "stale" when it matched nothing in THIS run. "Hasn't matched in
//! N runs" needs a run-history store that does not exist; it is a documented future extension.

use crate::allowlist::{AllowEntry, Allowlist};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashMap;

/// `check_id` of the findings this module emits.
pub(crate) const ALLOWLIST_CHECK_ID: &str = "allowlist";

pub(crate) struct Suppression<'a> {
    allowlist: &'a Allowlist,
    /// Parallel to `allowlist.entries()`: did this entry suppress at least one finding this run?
    matched: Vec<bool>,
    /// Checks that ran to completion this run -> how many raw findings each returned. Only an
    /// entry for one of these can be judged stale: a check that errored, or never ran, tells us
    /// nothing about whether its entries still match.
    completed: HashMap<String, usize>,
}

impl<'a> Suppression<'a> {
    pub fn new(allowlist: &'a Allowlist) -> Self {
        Self {
            allowlist,
            matched: vec![false; allowlist.entries().len()],
            completed: HashMap::new(),
        }
    }

    /// Records that `check_id` ran to completion and returned `raw_count` findings before
    /// suppression. Call once per check, with the number of findings passed to [`Self::apply`].
    pub fn mark_completed(&mut self, check_id: &str, raw_count: usize) {
        self.completed.insert(check_id.to_string(), raw_count);
    }

    /// Drops every finding an entry names (exact `check_id` + file + subject) and remembers which
    /// entries did so.
    pub fn apply(&mut self, raw: Vec<Finding>) -> Vec<Finding> {
        raw.into_iter()
            .filter(|f| {
                let mut suppressed = false;
                for (i, entry) in self.allowlist.entries().iter().enumerate() {
                    if names_finding(entry, f) {
                        self.matched[i] = true;
                        suppressed = true;
                    }
                }
                !suppressed
            })
            .collect()
    }

    /// One `Info` finding per entry that matched nothing in a check that completed, plus one per
    /// entry naming a check that is not registered at all (a typo; judged regardless of which
    /// checks ran).
    pub fn stale_findings(&self, registered: &[&str], project: &str) -> Vec<Finding> {
        let mut out = Vec::new();
        for (entry, matched) in self.allowlist.entries().iter().zip(&self.matched) {
            if *matched {
                continue;
            }
            if !registered.contains(&entry.check.as_str()) {
                out.push(unknown_check(entry, registered, project));
            } else if let Some(raw_count) = self.completed.get(&entry.check) {
                out.push(stale(entry, *raw_count, project));
            }
        }
        out
    }
}

fn names_finding(entry: &AllowEntry, f: &Finding) -> bool {
    entry.check == f.check_id
        && f.location
            .as_ref()
            .is_some_and(|l| l.file == entry.file.replace('\\', "/"))
        && f.subject.as_deref() == Some(entry.symbol.as_str())
}

fn base(entry: &AllowEntry, project: &str) -> Finding {
    Finding {
        check_id: ALLOWLIST_CHECK_ID.into(),
        severity: Severity::Info,
        confidence: Confidence::High,
        project: project.to_string(),
        location: Some(Location {
            file: ".coderipper.toml".into(),
            line: None,
        }),
        subject: Some(entry.symbol.clone()),
        summary: String::new(),
        detail: String::new(),
        positive_control: None,
    }
}

fn stale(entry: &AllowEntry, raw_count: usize, project: &str) -> Finding {
    Finding {
        summary: format!(
            "allowlist entry for `{}` in `{}` ({}) suppressed nothing this run -- its cause may be gone",
            entry.symbol, entry.file, entry.check
        ),
        detail: format!(
            "This entry was written because: \"{}\". Nothing `{}` reported this run matches it \
             (same check, same file, same symbol), so the suppression may no longer apply. Remove \
             it if the cause is fixed; keep it only if the finding is expected to come back.",
            entry.reason, entry.check
        ),
        positive_control: Some(format!(
            "check `{}` ran to completion this run and returned {raw_count} raw finding(s) before \
             suppression; none of them matched this entry",
            entry.check
        )),
        ..base(entry, project)
    }
}

fn unknown_check(entry: &AllowEntry, registered: &[&str], project: &str) -> Finding {
    Finding {
        summary: format!(
            "allowlist entry for `{}` names a check, `{}`, that is not registered",
            entry.symbol, entry.check
        ),
        detail: format!(
            "Registered checks: {}. An entry for a check that does not exist can never suppress \
             anything -- usually a typo in `check = \"...\"`. Reason given: \"{}\".",
            registered.join(", "),
            entry.reason
        ),
        positive_control: Some(format!(
            "this build's registered checks are: {}",
            registered.join(", ")
        )),
        ..base(entry, project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist(toml: &str) -> Allowlist {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".coderipper.toml"), toml).unwrap();
        Allowlist::load(tmp.path()).unwrap()
    }

    fn finding(check: &str, file: &str, subject: Option<&str>) -> Finding {
        Finding {
            check_id: check.into(),
            severity: Severity::Medium,
            confidence: Confidence::Medium,
            project: "p".into(),
            location: Some(Location {
                file: file.into(),
                line: Some(1),
            }),
            subject: subject.map(str::to_string),
            summary: "s".into(),
            detail: "d".into(),
            positive_control: None,
        }
    }

    const ONE: &str =
        "[[allow]]\ncheck = \"a\"\nfile = \"src/x.rs\"\nsymbol = \"f\"\nreason = \"why\"\n";

    #[test]
    fn a_finding_is_suppressed_only_on_an_exact_check_file_and_subject_match() {
        let al = allowlist(ONE);
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![
            finding("a", "src/x.rs", Some("f")), // exact: suppressed
            finding("b", "src/x.rs", Some("f")), // other check
            finding("a", "src/y.rs", Some("f")), // other file
            finding("a", "src/x.rs", Some("g")), // other symbol
            finding("a", "src/x.rs", None),      // no subject: can never be allowlisted
        ]);
        let kept: Vec<_> = kept
            .iter()
            .map(|f| (f.check_id.as_str(), f.subject.as_deref()))
            .collect();
        assert_eq!(
            kept,
            vec![
                ("b", Some("f")),
                ("a", Some("f")), // src/y.rs
                ("a", Some("g")),
                ("a", None),
            ]
        );
    }

    #[test]
    fn a_matched_entry_is_not_stale_and_an_unmatched_one_is() {
        let al = allowlist(&format!("{ONE}\n{}", ONE.replace("\"f\"", "\"gone\"")));
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        s.mark_completed("a", 1);
        assert!(kept.is_empty());
        let stale = s.stale_findings(&["a"], "p");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].subject.as_deref(), Some("gone"));
        assert_eq!(stale[0].severity, Severity::Info);
        assert!(stale[0]
            .positive_control
            .as_deref()
            .unwrap()
            .contains("1 raw finding"));
    }

    #[test]
    fn an_entry_for_a_check_that_did_not_complete_is_not_judged() {
        let al = allowlist(ONE);
        let s = Suppression::new(&al); // `a` never marked completed
        assert!(s.stale_findings(&["a"], "p").is_empty());
    }

    #[test]
    fn an_unregistered_check_is_flagged_whether_or_not_anything_ran() {
        let al = allowlist(ONE);
        let s = Suppression::new(&al);
        let out = s.stale_findings(&["reachability", "unused-return-values"], "p");
        assert_eq!(out.len(), 1);
        assert!(out[0].summary.contains("not registered"));
        assert!(out[0].detail.contains("reachability, unused-return-values"));
    }

    #[test]
    fn an_entry_written_with_windows_separators_still_matches() {
        // Findings always carry forward-slash paths; a Windows author may type `src\x.rs`.
        // A TOML literal string ('...') is how a Windows author keeps the backslash unescaped.
        let al = allowlist(&ONE.replace("\"src/x.rs\"", "'src\\x.rs'"));
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        assert!(kept.is_empty());
    }

    #[test]
    fn two_entries_for_the_same_finding_both_count_as_matched() {
        let al = allowlist(&format!("{ONE}\n{ONE}"));
        let mut s = Suppression::new(&al);
        s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        s.mark_completed("a", 1);
        assert!(s.stale_findings(&["a"], "p").is_empty());
    }
}
````

- [ ] **Step 4: Run the tests.** Run: `cargo test --lib suppression`
  Expected: PASS, 6 tests: `a_finding_is_suppressed_only_on_an_exact_check_file_and_subject_match`, `a_matched_entry_is_not_stale_and_an_unmatched_one_is`, `an_entry_for_a_check_that_did_not_complete_is_not_judged`, `an_unregistered_check_is_flagged_whether_or_not_anything_ran`, `an_entry_written_with_windows_separators_still_matches`, `two_entries_for_the_same_finding_both_count_as_matched`. Expect `dead_code` warnings for the not-yet-used module; Task 3 removes them.

- [ ] **Step 5: Prove two guards can fail.** (a) In `names_finding`, replace `&& f.subject.as_deref() == Some(entry.symbol.as_str())` with `&& true`: `a_finding_is_suppressed_only_on_an_exact_check_file_and_subject_match` and `a_matched_entry_is_not_stale_and_an_unmatched_one_is` must FAIL. (b) In `stale_findings`, make the `else if let Some(raw_count) = self.completed.get(...)` branch unconditional (`else { let raw_count = ...copied().unwrap_or(0); out.push(stale(...)) }`): `an_entry_for_a_check_that_did_not_complete_is_not_judged` must FAIL. (c) Change `entry.file.replace('\\', "/")` back to `entry.file`: `an_entry_written_with_windows_separators_still_matches` must FAIL with an assertion (not a TOML parse panic — if it panics in `allowlist()`, the test is wrong, not the code). Revert each.

- [ ] **Step 6: Format and commit** — `cargo fmt && git add -A src && git commit -m "feat: suppression module -- exact-identity matching and stale-entry findings"`.

---

### Task 3: The host applies the allowlist; checks return raw findings

**Files:** Create `tests/allowlist.rs`; modify `src/lib.rs`, `src/check.rs`, `src/checks/reachability/mod.rs`, `src/checks/unused_return_values/mod.rs`, `tests/reachability.rs`, `tests/unused_return_values.rs`.

**Interfaces:**
- Consumes: Task 2's `Suppression`, `Allowlist::{load, empty, entries}`.
- Produces: `run_checks` behaviour: allowlist loaded once from `ctx.project_root`; each completed check's findings pass through `Suppression::apply`; a check is `mark_completed` only if it returned `Ok` and every finding validated; stale/unknown-check findings are appended at the end. `Check::run` now returns RAW findings.

- [ ] **Step 1: Write the failing host-level tests first.** Create `tests/allowlist.rs`:

````rust
// tests/allowlist.rs
//! Host-level allowlist behaviour: suppression is applied by `run_checks`, not by each check, so
//! the host can see which entries matched nothing ("stale this run", design doc §4).

mod common;

use coderipper::check::{CheckContext, Tier};
use coderipper::finding::{Finding, Severity};
use coderipper::{run_checks, RunResult};
use common::{git_repo_with, MANIFEST};

const URV: &str = "unused-return-values";

fn ctx(repo: &tempfile::TempDir) -> CheckContext {
    CheckContext {
        project_root: repo.path().to_path_buf(),
        portfolio_root: repo.path().to_path_buf(),
    }
}

fn run_one(repo: &tempfile::TempDir, check_id: &str) -> RunResult {
    run_checks(&ctx(repo), Tier::Fast, Some(check_id))
}

fn entry(check: &str, file: &str, symbol: &str) -> String {
    format!("[[allow]]\ncheck = \"{check}\"\nfile = \"{file}\"\nsymbol = \"{symbol}\"\nreason = \"test\"\n\n")
}

/// A crate with exactly one finding for `unused-return-values`: `f`, discarded in `main`.
fn crate_with_one_finding(allowlist: &str) -> tempfile::TempDir {
    git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
        (".coderipper.toml", allowlist),
    ])
}

fn stale(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|f| f.check_id == "allowlist")
        .collect()
}

#[test]
fn a_matching_entry_suppresses_the_finding_and_is_not_reported_stale() {
    let repo = crate_with_one_finding(&entry(URV, "src/main.rs", "f"));
    let result = run_one(&repo, URV);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

#[test]
fn without_an_entry_the_finding_is_reported() {
    let repo = crate_with_one_finding("");
    let result = run_one(&repo, URV);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].subject.as_deref(), Some("f"));
}

#[test]
fn an_entry_whose_symbol_matches_nothing_is_reported_as_an_informational_finding() {
    let repo = crate_with_one_finding(&format!(
        "{}{}",
        entry(URV, "src/main.rs", "f"),
        entry(URV, "src/main.rs", "long_since_deleted")
    ));
    let result = run_one(&repo, URV);

    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let stale = stale(&result.findings);
    assert_eq!(stale.len(), 1, "{:?}", result.findings);
    let s = stale[0];
    assert_eq!(s.severity, Severity::Info);
    assert_eq!(s.subject.as_deref(), Some("long_since_deleted"));
    assert!(s.summary.contains("`long_since_deleted`"), "{}", s.summary);
    assert!(
        s.detail.contains("test"),
        "the entry's reason must be shown: {}",
        s.detail
    );
    assert!(s.positive_control.is_some());
    assert!(s.clone().validate().is_ok());
}

#[test]
fn a_matching_symbol_in_a_different_file_does_not_suppress_and_the_entry_is_stale() {
    // Exact identity, not loose containment: same symbol, wrong file.
    let repo = crate_with_one_finding(&entry(URV, "src/other.rs", "f"));
    let result = run_one(&repo, URV);
    assert_eq!(
        result.findings.iter().filter(|f| f.check_id == URV).count(),
        1
    );
    assert_eq!(stale(&result.findings).len(), 1);
}

#[test]
fn an_entry_for_a_check_that_failed_to_run_is_not_judged() {
    // The build is broken, so the check errors. Its entries must not be called stale: nothing was
    // learned about whether they still match.
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/main.rs", "fn main() { let x: i32 = \"no\"; }\n"),
        (".coderipper.toml", &entry(URV, "src/main.rs", "f")),
    ]);
    let result = run_one(&repo, URV);
    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    assert!(stale(&result.findings).is_empty(), "{:?}", result.findings);
}

#[test]
fn an_entry_for_a_registered_check_that_did_not_run_is_not_judged() {
    // `--check reachability` never ran URV, so a URV entry says nothing about this run.
    let repo = crate_with_one_finding(&entry(URV, "src/main.rs", "f"));
    let result = run_one(&repo, "reachability");
    assert!(stale(&result.findings).is_empty(), "{:?}", result.findings);
}

#[test]
fn an_entry_naming_an_unknown_check_is_reported_even_when_that_check_was_not_asked_for() {
    let repo = crate_with_one_finding(&entry("unused-return-valeus", "src/main.rs", "f"));
    let result = run_one(&repo, URV);
    let stale = stale(&result.findings);
    assert_eq!(stale.len(), 1, "{:?}", result.findings);
    assert!(
        stale[0].summary.contains("unused-return-valeus"),
        "{}",
        stale[0].summary
    );
    assert!(
        stale[0].detail.contains(URV),
        "should list registered checks: {}",
        stale[0].detail
    );
    assert!(stale[0].clone().validate().is_ok());
}

#[test]
fn a_malformed_allowlist_is_an_error_but_does_not_swallow_the_findings() {
    let repo = crate_with_one_finding("[[allow]]\ncheck = \"x\"\n");
    let result = run_one(&repo, URV);
    assert!(
        result.errors.iter().any(|e| e.contains("allowlist")),
        "{:?}",
        result.errors
    );
    assert_eq!(
        result.findings.iter().filter(|f| f.check_id == URV).count(),
        1
    );
}
````

  Run: `cargo test --test allowlist`
  Expected: FAIL — 4 of 8 fail: `an_entry_whose_symbol_matches_nothing_is_reported_as_an_informational_finding`, `a_matching_symbol_in_a_different_file_does_not_suppress_and_the_entry_is_stale`, `an_entry_naming_an_unknown_check_is_reported_even_when_that_check_was_not_asked_for`, `a_malformed_allowlist_is_an_error_but_does_not_swallow_the_findings`. The other four **pass already** — they are guards against an over-eager implementation (not-judged cases, and plain suppression), and must still pass after Step 2.

- [ ] **Step 2: Wire it in.** Apply this diff. It (a) rewrites `run_checks`, (b) removes the allowlist code from both checks, (c) turns the two per-check "an allowlisted symbol is suppressed" tests into host-level ones (`tests/reachability.rs`) or drops the duplicate (`tests/unused_return_values.rs`; covered by `tests/allowlist.rs`):

````diff
diff --git a/src/checks/reachability/mod.rs b/src/checks/reachability/mod.rs
--- a/src/checks/reachability/mod.rs
+++ b/src/checks/reachability/mod.rs
@@ -22,7 +22,6 @@ mod diagnostics;
 mod rewriter;
 mod sentinel;
 
-use crate::allowlist::Allowlist;
 use crate::check::{Check, CheckContext, Network, Scope};
 use crate::finding::{Confidence, Finding, Location, Severity};
 use crate::worktree::RewrittenWorktree;
@@ -95,7 +94,6 @@ impl Check for ReachabilityCheck {
              so this run's result can't be trusted."
         );
 
-        let allowlist = Allowlist::load(&ctx.project_root)?;
         let project_name = ctx
             .project_root
             .file_name()
@@ -106,7 +104,6 @@ impl Check for ReachabilityCheck {
             .hits
             .into_iter()
             .filter(|hit| hit.symbol != SENTINEL_SYMBOL) // the sentinel itself is not a real finding
-            .filter(|hit| !allowlist.is_allowed("reachability", &hit.file, &hit.symbol))
             .map(|hit| Finding {
                 check_id: "reachability".into(),
                 severity: Severity::Medium,
diff --git a/src/checks/unused_return_values/mod.rs b/src/checks/unused_return_values/mod.rs
--- a/src/checks/unused_return_values/mod.rs
+++ b/src/checks/unused_return_values/mod.rs
@@ -21,7 +21,6 @@ mod classify;
 mod rewriter;
 mod sentinel;
 
-use crate::allowlist::Allowlist;
 use crate::cargo_json::build_all_targets;
 use crate::check::{Check, CheckContext, Network, Scope};
 use crate::finding::{Confidence, Finding, Location, Severity};
@@ -92,7 +91,6 @@ impl Check for UnusedReturnValuesCheck {
              can't be trusted."
         );
 
-        let allowlist = Allowlist::load(&ctx.project_root)?;
         let project = ctx
             .project_root
             .file_name()
@@ -108,7 +106,6 @@ impl Check for UnusedReturnValuesCheck {
         Ok(classification
             .usages
             .iter()
-            .filter(|u| !allowlist.is_allowed(CHECK_ID, &u.tag.file, &u.tag.name))
             .map(|u| {
                 let (severity, summary) = if u.all_ignored() {
                     (
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -6,8 +6,10 @@ pub mod finding;
 pub(crate) mod suppression;
 pub(crate) mod worktree;
 
+use allowlist::Allowlist;
 use check::{Check, CheckContext, Tier};
 use finding::Finding;
+use suppression::Suppression;
 
 /// Every compiled-in check, in the order they run.
 ///
@@ -21,7 +23,9 @@ pub fn registered_checks() -> Vec<Box<dyn Check>> {
 }
 
 /// Run every registered check at or below the requested tier, collect and validate their
-/// findings, and return them. A check whose `run` returns an invalid absence-claim finding is
+/// findings, apply the project's allowlist to them (checks return RAW findings; suppression is the
+/// host's job, see `suppression`), and return what is left plus one `Info` finding per allowlist
+/// entry that no longer suppresses anything. A check whose `run` returns an invalid absence-claim finding is
 /// dropped with an error noted in `errors`, not silently included — see `Finding::validate`.
 pub struct RunResult {
     pub findings: Vec<Finding>,
@@ -32,7 +36,17 @@ pub fn run_checks(ctx: &CheckContext, tier: Tier, only_check_id: Option<&str>) -
     let mut findings = Vec::new();
     let mut errors = Vec::new();
 
-    for check in registered_checks() {
+    // A malformed allowlist must not swallow the findings: report it, and run unsuppressed.
+    let allowlist = Allowlist::load(&ctx.project_root).unwrap_or_else(|e| {
+        errors.push(format!("allowlist: {e}"));
+        Allowlist::empty()
+    });
+    let mut suppression = Suppression::new(&allowlist);
+
+    let checks = registered_checks();
+    let registered: Vec<&str> = checks.iter().map(|c| c.id()).collect();
+
+    for check in &checks {
         if let Some(id) = only_check_id {
             if check.id() != id {
                 continue;
@@ -44,17 +58,35 @@ pub fn run_checks(ctx: &CheckContext, tier: Tier, only_check_id: Option<&str>) -
 
         match check.run(ctx) {
             Ok(raw_findings) => {
+                let mut valid = Vec::new();
+                let mut all_valid = true;
                 for f in raw_findings {
                     match f.validate() {
-                        Ok(valid) => findings.push(valid),
-                        Err(e) => errors.push(format!("{}: {e}", check.id())),
+                        Ok(f) => valid.push(f),
+                        Err(e) => {
+                            all_valid = false;
+                            errors.push(format!("{}: {e}", check.id()));
+                        }
                     }
                 }
+                // A check that produced an invalid finding is not trusted to have completed, so
+                // its allowlist entries are not judged either.
+                if all_valid {
+                    suppression.mark_completed(check.id(), valid.len());
+                }
+                findings.extend(suppression.apply(valid));
             }
             Err(e) => errors.push(format!("{} failed to run: {e}", check.id())),
         }
     }
 
+    let project = ctx
+        .project_root
+        .file_name()
+        .map(|n| n.to_string_lossy().to_string())
+        .unwrap_or_default();
+    findings.extend(suppression.stale_findings(&registered, &project));
+
     RunResult { findings, errors }
 }
 
diff --git a/tests/reachability.rs b/tests/reachability.rs
--- a/tests/reachability.rs
+++ b/tests/reachability.rs
@@ -97,8 +97,11 @@ reason = "test: confirm the allowlist suppresses a real finding"
         project_root: repo.path().to_path_buf(),
         portfolio_root: repo.path().to_path_buf(),
     };
-    let findings = ReachabilityCheck.run(&ctx).unwrap();
-    assert!(findings.is_empty());
+    // Suppression is the host's job: the check itself returns the raw finding.
+    assert_eq!(ReachabilityCheck.run(&ctx).unwrap().len(), 1);
+    let result = coderipper::run_checks(&ctx, coderipper::check::Tier::Fast, Some("reachability"));
+    assert!(result.errors.is_empty(), "{:?}", result.errors);
+    assert!(result.findings.is_empty(), "{:?}", result.findings);
 }
 
 fn standalone_git_repo(cargo_toml: &str, main_rs: &str) -> tempfile::TempDir {
diff --git a/tests/unused_return_values.rs b/tests/unused_return_values.rs
--- a/tests/unused_return_values.rs
+++ b/tests/unused_return_values.rs
@@ -108,18 +108,6 @@ fn a_function_that_only_discards_its_own_recursive_result_is_not_flagged() {
     assert!(run(&repo).unwrap().is_empty());
 }
 
-#[test]
-fn an_allowlisted_function_is_suppressed() {
-    let repo = git_repo_with(&[
-        ("Cargo.toml", MANIFEST),
-        ("src/main.rs", "fn f() -> i32 { 1 }\nfn main() { f(); }\n"),
-        (
-            ".coderipper.toml",
-            "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nsymbol = \"f\"\nreason = \"test\"\n",
-        ),
-    ]);
-    assert!(run(&repo).unwrap().is_empty());
-}
 #[test]
 fn a_crate_wide_allow_of_either_lint_cannot_hide_a_finding() {
     // Was an error ("blind crate") before the review: `--force-warn` now makes both lints fire
````

- [ ] **Step 3: Document the contract change** on `Check::run` in `src/check.rs` — replace its doc comment with:

```rust
    /// Run the check and return whatever it found, RAW: do not apply the project's allowlist. The
    /// host suppresses (see `suppression`) because only it can tell which allowlist entries went
    /// stale. A check that would make an absence claim without a positive control must not
    /// construct that `Finding` at all — see `Finding::validate`, which the host calls on every
    /// finding before it reaches a report. Set `Finding::subject` to the symbol the finding is
    /// about, or it can never be allowlisted.
```

- [ ] **Step 4: Run everything.** Run: `cargo fmt && cargo test --no-fail-fast`
  Expected: PASS — 67 lib, 8 in `tests/allowlist.rs`, 5 cli, 4 reachability, 13 unused-return-values (14 minus the dropped duplicate).

- [ ] **Step 5: Prove the host-level guards can fail.** In `lib.rs`, change `if all_valid { suppression.mark_completed(...) }` to call `mark_completed` unconditionally AND in the `Err(e)` arm too: `an_entry_for_a_check_that_failed_to_run_is_not_judged` must FAIL. Revert. Then make `run_checks` skip `findings.extend(suppression.stale_findings(...))`: the three stale/unknown tests must FAIL. Revert.

- [ ] **Step 6: Lint and commit.** Run: `cargo clippy --all-targets -- -D warnings` (expect no output; the `dead_code` warnings from Task 2 are gone).
  `git add -A src tests && git commit -m "feat: the host applies the allowlist and reports stale entries; checks return raw findings"`.

---

### Task 4: CLI test, README, spec

**Files:** Modify `tests/cli.rs`, `README.md`, `docs/superpowers/specs/2026-09-30-audit-host-design.md`.

- [ ] **Step 1: Pin the end-to-end behaviour and update the docs.** Apply:

````diff
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -23,6 +23,24 @@ implemented: `reachability` (dead code, including `pub` items) and `unused-retur
 return value every caller discards). See `docs/superpowers/specs/2026-09-30-audit-host-design.md` for the design, and
 `docs/superpowers/plans/` for what's actually being built and in what order.
 
+## Suppressing a finding
+
+Some findings are deliberate (public API built ahead of its consumer, a value discarded on purpose).
+List them in `.coderipper.toml` at the project root. Every entry names the check, the file, and the
+symbol, and **must** say why:
+
+```toml
+[[allow]]
+check = "reachability"
+file = "src/api.rs"
+symbol = "public_entry_point"
+reason = "published crate API, consumed outside this repo"
+```
+
+An entry that suppressed nothing in a run is reported as an `Info` finding (check id `allowlist`), so a
+suppression whose cause has been fixed does not linger. An entry naming a check that does not exist is
+reported the same way. An entry for a check that failed or did not run in that run is not judged.
+
 ## Why
 
 Found by hand, once, during an unrelated design review: a fully correct, well-tested module in another
diff --git a/docs/superpowers/specs/2026-09-30-audit-host-design.md b/docs/superpowers/specs/2026-09-30-audit-host-design.md
--- a/docs/superpowers/specs/2026-09-30-audit-host-design.md
+++ b/docs/superpowers/specs/2026-09-30-audit-host-design.md
@@ -152,11 +152,15 @@ stated as a norm but not given a concrete mechanism. Formalizing it:
   suppression expires the same way — surfaced informationally, not silently dropped, so a human confirms
   the symptom is actually gone too rather than assuming the dependency guess was right.
 
-**Open question, not resolved here**: whether "permanent" vs "temporary" is really a separate axis from
-these two triggers, or whether "permanent" just means "self-dissolving only, no `until_fixed` link" and
-"temporary" means "has one." The latter is simpler — one suppression mechanism, one universal trigger
-(self-dissolving) plus one optional trigger (`until_fixed`) — and is the PM's lean, but it's genuinely
-open until a first implementation is attempted against it.
+**Implemented 2026-10-02 (`docs/superpowers/plans/2026-10-02-allowlist-stale-entries.md`), narrower than
+the above, by the PM's scoping call.** One axis, not two: the universal trigger only, and only in its
+**stale-this-run** form. The host (not each check) applies the allowlist to every check's raw findings and
+reports each entry that matched nothing *in this run* as an `Info` finding (check id `allowlist`; same
+`Finding` type, no parallel channel). An entry is judged only if its check ran to completion in that run; an
+entry naming an unregistered check is reported regardless. **Deliberately not built**: "hasn't matched in N
+runs" (needs a run-history store that doesn't exist; revisit if one is built for another reason) and the
+`until_fixed` link to a different finding. The "one axis or two" question is settled for now as one axis;
+`until_fixed` is a documented future extension.
 
 ## 5. Five more checks, sketched to pressure-test the interface above (not designed in full)
 
diff --git a/tests/cli.rs b/tests/cli.rs
--- a/tests/cli.rs
+++ b/tests/cli.rs
@@ -181,3 +181,50 @@ fn the_unused_return_values_check_runs_by_id_and_reports_a_finding() {
         .stdout(predicate::str::contains("(unused-return-values)"))
         .stdout(predicate::str::contains("`f`"));
 }
+
+#[test]
+fn a_stale_allowlist_entry_is_printed_as_an_informational_finding_and_does_not_fail_the_run() {
+    let tmp = tempfile::tempdir().unwrap();
+    std::fs::write(
+        tmp.path().join("Cargo.toml"),
+        "[package]\nname = \"tidy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
+    )
+    .unwrap();
+    std::fs::create_dir(tmp.path().join("src")).unwrap();
+    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
+    std::fs::write(
+        tmp.path().join(".coderipper.toml"),
+        "[[allow]]\ncheck = \"unused-return-values\"\nfile = \"src/main.rs\"\nsymbol = \"long_gone\"\nreason = \"was a real discard once\"\n",
+    )
+    .unwrap();
+    for args in [
+        vec!["init", "-q"],
+        vec!["add", "-A"],
+        vec![
+            "-c",
+            "user.email=t@t",
+            "-c",
+            "user.name=t",
+            "commit",
+            "-q",
+            "-m",
+            "init",
+        ],
+    ] {
+        StdCommand::new("git")
+            .args(args)
+            .current_dir(tmp.path())
+            .status()
+            .unwrap();
+    }
+
+    Command::cargo_bin("coderipper")
+        .unwrap()
+        .args(["check", "unused-return-values", "--project"])
+        .arg(tmp.path())
+        .assert()
+        .success()
+        .stdout(predicate::str::contains("[Info/High]"))
+        .stdout(predicate::str::contains("`long_gone`"))
+        .stdout(predicate::str::contains("(allowlist)"));
+}
````

- [ ] **Step 2: Run the CLI test.** Run: `cargo test --test cli`
  Expected: PASS, 6 tests. (This one passes the moment it is written — Task 3 already delivered the behaviour — so it is a pin, not a driver: confirm it can fail by deleting the `findings.extend(suppression.stale_findings(...))` line and watching `a_stale_allowlist_entry_is_printed_as_an_informational_finding_and_does_not_fail_the_run` FAIL, then revert.)

- [ ] **Step 3: Full CI sequence, exactly as CI runs it.**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --no-fail-fast
```

  Expected: every command exits 0; `cargo test` shows 67 lib, 8 allowlist, 6 cli, 4 reachability, 13 unused-return-values.

- [ ] **Step 4: Probe the real binary (no write into the shared checkout).** In a scratch dir, make a one-commit crate whose `src/main.rs` is `fn f() -> i32 { 1 }\nfn main() { f(); }` and whose `.coderipper.toml` has two entries for `unused-return-values` / `src/main.rs`: symbol `f` and symbol `long_gone`, then run `<built-binary> check unused-return-values --project <dir>`.
  Expected: exactly one line, `[Info/High] <dir-name> — allowlist entry for `long_gone` in `src/main.rs` (unused-return-values) suppressed nothing this run -- its cause may be gone (allowlist)`, exit status 0. Then delete the `f` entry and re-run: expected two lines — the `unused-return-values` finding for `f`, and the stale line.

- [ ] **Step 5: Commit, push, report.** `git add -A README.md docs tests && git commit -m "test+docs: CLI pin for stale entries, README section, spec section 4 updated"`, push the branch, and report to the PM (the lane account cannot open or merge PRs; the PM does). Read the PR's checks and **unresolved review threads** (GraphQL `reviewThreads{isResolved}`) before calling it READY.
