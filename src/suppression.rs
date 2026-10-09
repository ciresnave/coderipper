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
use crate::finding::{Confidence, Finding, Location, Severity, Waived};
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
    /// The findings entries waived, each with the reason of the first entry that named it.
    waived: Vec<Waived>,
}

impl<'a> Suppression<'a> {
    pub fn new(allowlist: &'a Allowlist) -> Self {
        Self {
            allowlist,
            matched: vec![false; allowlist.entries().len()],
            completed: HashMap::new(),
            waived: Vec::new(),
        }
    }

    /// Records that `check_id` ran to completion and returned `raw_count` findings before
    /// suppression. Call once per check, with the number of findings passed to [`Self::apply`].
    pub fn mark_completed(&mut self, check_id: &str, raw_count: usize) {
        self.completed.insert(check_id.to_string(), raw_count);
    }

    /// Takes out every finding an entry names (exact `check_id` + file, and the symbol and lines when the entry gives them),
    /// remembers which entries did so, and keeps the waived findings with their reasons (see [`Self::take_waived`]).
    pub fn apply(&mut self, raw: Vec<Finding>) -> Vec<Finding> {
        let mut kept = Vec::new();
        for f in raw {
            let mut reason = None;
            for (i, entry) in self.allowlist.entries().iter().enumerate() {
                if names_finding(entry, &f) {
                    self.matched[i] = true;
                    reason.get_or_insert_with(|| entry.reason.clone());
                }
            }
            match reason {
                Some(reason) => self.waived.push(Waived { finding: f, reason }),
                None => kept.push(f),
            }
        }
        kept
    }

    /// The findings waived so far, in the order they were seen.
    pub fn take_waived(&mut self) -> Vec<Waived> {
        std::mem::take(&mut self.waived)
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
    let Some(location) = f.location.as_ref() else {
        return false;
    };
    entry.check == f.check_id
        && location.file == normalize_entry_path(&entry.file)
        && entry
            .symbol
            .as_deref()
            .is_none_or(|symbol| f.subject.as_deref() == Some(symbol))
        && entry
            .lines
            .is_none_or(|range| location.line.is_some_and(|line| range.contains(line)))
}

/// What an entry names, for messages: the symbol when it gives one, else its lines, else just the file.
fn target(entry: &AllowEntry) -> String {
    match (&entry.symbol, entry.lines) {
        (Some(symbol), _) => format!("`{symbol}`"),
        (None, Some(lines)) => format!("lines {lines}"),
        (None, None) => "every finding".to_string(),
    }
}

/// Findings carry `src/x.rs`; an author may write `src\x.rs`, `./src/x.rs` or `src//x.rs`. Compared
/// by path component, so none of those makes a live suppression look stale. Case is NOT folded.
fn normalize_entry_path(path: &str) -> String {
    path.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
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
        subject: entry.symbol.clone(),
        summary: String::new(),
        detail: String::new(),
        positive_control: None,
        member: None,
    }
}

fn stale(entry: &AllowEntry, raw_count: usize, project: &str) -> Finding {
    Finding {
        summary: format!(
            "allowlist entry for {} in `{}` ({}) suppressed nothing this run -- its cause may be gone",
            target(entry),
            entry.file,
            entry.check
        ),
        detail: format!(
            "This entry was written because: \"{}\". Nothing `{}` reported this run matches it \
             (same check, same file, and the same symbol and lines when the entry gives them), so the suppression may no longer apply. Remove \
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
            "allowlist entry for {} in `{}` names a check, `{}`, that is not registered",
            target(entry),
            entry.file,
            entry.check
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
            member: None,
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
    fn dot_slash_and_doubled_separators_in_an_entry_still_match() {
        // Review finding: `./src/x.rs` and `src//x.rs` suppressed nothing, and the entry was then
        // reported as "its cause may be gone" right beside the finding it was meant to suppress.
        for spelling in ["./src/x.rs", "src//x.rs", "./src//x.rs"] {
            let al = allowlist(&ONE.replace("src/x.rs", spelling));
            let mut s = Suppression::new(&al);
            let kept = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
            assert!(kept.is_empty(), "entry spelled {spelling:?} should match");
        }
    }

    fn at_line(mut f: Finding, line: Option<u32>) -> Finding {
        f.location.as_mut().unwrap().line = line;
        f
    }

    const NO_SYMBOL: &str = "[[allow]]\ncheck = \"a\"\nfile = \"src/x.rs\"\nreason = \"why\"\n";

    #[test]
    fn an_entry_without_a_symbol_waives_every_finding_of_that_check_in_that_file() {
        let al = allowlist(NO_SYMBOL);
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![
            finding("a", "src/x.rs", Some("f")), // waived
            finding("a", "src/x.rs", None),      // waived: no subject needed
            finding("a", "src/y.rs", Some("f")), // other file
            finding("b", "src/x.rs", Some("f")), // other check
        ]);
        assert_eq!(kept.len(), 2);
        assert_eq!(s.take_waived().len(), 2);
    }

    #[test]
    fn a_finding_with_no_location_is_never_waived() {
        let al = allowlist(NO_SYMBOL);
        let mut s = Suppression::new(&al);
        let mut f = finding("a", "src/x.rs", None);
        f.location = None;
        assert_eq!(s.apply(vec![f]).len(), 1);
        assert!(s.take_waived().is_empty());
    }

    #[test]
    fn a_lines_entry_waives_only_findings_at_those_lines() {
        let al = allowlist(&format!("{NO_SYMBOL}lines = \"10-20\"\n"));
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![
            at_line(finding("a", "src/x.rs", Some("f")), Some(9)),
            at_line(finding("a", "src/x.rs", Some("f")), Some(10)),
            at_line(finding("a", "src/x.rs", Some("f")), Some(20)),
            at_line(finding("a", "src/x.rs", Some("f")), Some(21)),
            at_line(finding("a", "src/x.rs", Some("f")), None), // names no line: never inside a range
        ]);
        let kept: Vec<_> = kept
            .iter()
            .map(|f| f.location.as_ref().unwrap().line)
            .collect();
        assert_eq!(kept, vec![Some(9), Some(21), None]);
        assert_eq!(s.take_waived().len(), 2);
    }

    #[test]
    fn a_symbol_and_lines_must_both_match() {
        let al = allowlist(&format!("{ONE}lines = \"5\"\n"));
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![
            at_line(finding("a", "src/x.rs", Some("f")), Some(5)), // both: waived
            at_line(finding("a", "src/x.rs", Some("f")), Some(6)), // symbol only
            at_line(finding("a", "src/x.rs", Some("g")), Some(5)), // line only
        ]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn when_two_entries_name_a_finding_the_first_ones_reason_is_kept() {
        let second = ONE.replace("\"why\"", "\"second\"");
        let al = allowlist(&format!("{ONE}\n{second}"));
        let mut s = Suppression::new(&al);
        s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        let waived = s.take_waived();
        assert_eq!(waived.len(), 1, "one finding is waived once");
        assert_eq!(waived[0].reason, "why");
    }

    #[test]
    fn a_waived_finding_is_kept_whole_with_the_entrys_reason() {
        let al = allowlist(ONE);
        let mut s = Suppression::new(&al);
        s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        let waived = s.take_waived();
        assert_eq!(waived.len(), 1);
        assert_eq!(waived[0].reason, "why");
        assert_eq!(waived[0].finding.subject.as_deref(), Some("f"));
        assert!(s.take_waived().is_empty(), "taking empties the list");
    }

    #[test]
    fn a_stale_entry_without_a_symbol_says_what_it_names() {
        let al = allowlist(NO_SYMBOL);
        let mut s = Suppression::new(&al);
        s.mark_completed("a", 0);
        let stale = s.stale_findings(&["a"], "p");
        assert_eq!(stale.len(), 1);
        assert!(
            stale[0].summary.contains("every finding"),
            "{}",
            stale[0].summary
        );
        assert_eq!(stale[0].subject, None);
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
