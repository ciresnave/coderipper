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
