// SPDX-License-Identifier: MIT OR Apache-2.0
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
use std::path::{Path, PathBuf};

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
    /// The project root, when findings under its conformance fixtures are not to be reported (see [`is_fixture`]).
    fixtures_of: Option<PathBuf>,
    /// How many findings were left out as fixture findings.
    excluded: usize,
}

impl<'a> Suppression<'a> {
    pub fn new(allowlist: &'a Allowlist) -> Self {
        Self {
            allowlist,
            matched: vec![false; allowlist.entries().len()],
            completed: HashMap::new(),
            waived: Vec::new(),
            fixtures_of: None,
            excluded: 0,
        }
    }

    /// Like [`Self::new`], and a finding located in one of `project_root`'s own conformance fixtures is left out of the
    /// result, not reported: the fixture is defective on purpose, and the repository that holds it is not the project the
    /// fixture describes.
    pub fn for_project(allowlist: &'a Allowlist, project_root: &Path) -> Self {
        Self {
            fixtures_of: Some(project_root.to_path_buf()),
            ..Self::new(allowlist)
        }
    }

    /// How many findings [`Self::apply`] left out because they were in a conformance fixture; zero for [`Self::new`].
    pub fn excluded(&self) -> usize {
        self.excluded
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
            if self.in_fixture(&f) {
                self.excluded += 1;
                continue;
            }
            let mut reason = None;
            for (i, entry) in self.allowlist.entries().iter().enumerate() {
                if names_finding(entry, &f) {
                    self.matched[i] = true;
                    reason.get_or_insert_with(|| {
                        (
                            entry.reason.clone(),
                            entry.source.clone(),
                            entry.declared_public,
                        )
                    });
                }
            }
            match reason {
                Some((reason, source, declared_public)) => self.waived.push(Waived {
                    finding: f,
                    reason,
                    source,
                    declared_public,
                }),
                None => kept.push(f),
            }
        }
        kept
    }

    fn in_fixture(&self, f: &Finding) -> bool {
        let (Some(root), Some(location)) = (self.fixtures_of.as_deref(), f.location.as_ref())
        else {
            return false;
        };
        is_fixture(root, &location.file)
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
            } else if entry.source.is_some() {
                // a wide entry that matches nothing here is not stale: it may be written for another repository
            } else if let Some(raw_count) = self.completed.get(&entry.check) {
                out.push(stale(entry, *raw_count, project));
            }
        }
        out
    }
}

/// Is `file` (a `/`-separated path relative to `root`) inside a conformance fixture of `root`: `conformance/<rule>/<variant>/...`
/// where the variant directory holds the fixture's `expect.toml`? A same-shaped file anywhere else, or under `conformance/` but not
/// in a fixture (no `expect.toml`), is not one.
fn is_fixture(root: &Path, file: &str) -> bool {
    let mut parts = file.split('/');
    let (Some("conformance"), Some(rule), Some(variant), Some(_)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if [rule, variant].iter().any(|p| p.is_empty() || *p == "..") {
        return false;
    }
    root.join("conformance")
        .join(rule)
        .join(variant)
        .join("expect.toml")
        .is_file()
}

fn names_finding(entry: &AllowEntry, f: &Finding) -> bool {
    let Some(location) = f.location.as_ref() else {
        return false;
    };
    let file_matches = if entry.declared_public {
        glob_match(&normalize_entry_path(&entry.file), &location.file)
    } else {
        location.file == normalize_entry_path(&entry.file)
    };
    // a crate-level declaration covers a package: its findings carry the package as `member` in a workspace run, else as `project`
    let crate_matches = entry.run_scoped
        || entry
            .crate_scope
            .as_deref()
            .is_none_or(|name| match &f.member {
                Some(member) => member == name,
                None => f.project == name,
            });
    crate_matches
        && entry.check == f.check_id
        && file_matches
        && entry
            .symbol
            .as_deref()
            .is_none_or(|symbol| f.subject.as_deref() == Some(symbol))
        && entry
            .lines
            .is_none_or(|range| location.line.is_some_and(|line| range.contains(line)))
}

/// Glob match of a `/`-separated path: `*` and `?` stay inside one path component, a component that is exactly `**` matches
/// any number of components (none included). Case is not folded, as for plain paths.
fn glob_match(pattern: &str, path: &str) -> bool {
    fn component(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => component(&p[1..], s) || (!s.is_empty() && component(p, &s[1..])),
            (Some('?'), Some(_)) => component(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => component(&p[1..], &s[1..]),
            _ => false,
        }
    }
    fn walk(p: &[&str], s: &[&str]) -> bool {
        match p.split_first() {
            None => s.is_empty(),
            Some((&"**", rest)) => (0..=s.len()).any(|skip| walk(rest, &s[skip..])),
            Some((head, rest)) => match s.split_first() {
                Some((first, tail)) => {
                    let h: Vec<char> = head.chars().collect();
                    let f: Vec<char> = first.chars().collect();
                    component(&h, &f) && walk(rest, tail)
                }
                None => false,
            },
        }
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    walk(&p, &s)
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
        claim: crate::finding::Claim::Presence,
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

/// A workspace root's `public_api_crates` entry whose `crate` is no package of the workspace: it can never cover anything, which is
/// nearly always a misspelt name (it is the cargo package name, not the directory).
pub(crate) fn unknown_crate(entry: &AllowEntry, packages: &[&str], project: &str) -> Finding {
    let name = entry.crate_scope.as_deref().unwrap_or_default();
    Finding {
        summary: format!(
            "`[reachability] public_api_crates` entry names `{name}`, which is no package of this workspace"
        ),
        detail: format!(
            "This declaration was written because: \"{}\". The workspace's packages are: {}. The name is the cargo package name (the              `name` in the member's `Cargo.toml`), not its directory. Fix the spelling, or remove the entry if the crate is gone.",
            entry.reason,
            packages.join(", ")
        ),
        positive_control: Some(format!(
            "the workspace was read whole ({} packages) and none is named `{name}`",
            packages.len()
        )),
        ..base(entry, project)
    }
}

fn stale(entry: &AllowEntry, raw_count: usize, project: &str) -> Finding {
    if let Some(name) = &entry.crate_scope {
        return Finding {
            summary: format!(
                "`[reachability] public_api_crates` entry for crate `{name}` matched nothing this run -- the package name may be wrong, or it has no unreferenced public item"
            ),
            detail: format!(
                "This declaration was written because: \"{}\". No `reachability` finding this run belongs to a package named `{name}`, so it declares                  nothing. Fix the name if it is misspelt (it is the cargo package name, not the directory), remove it if the crate is gone, or                  keep it if the crate has no unreferenced public item today.",
                entry.reason
            ),
            positive_control: Some(format!(
                "check `reachability` ran to completion this run and returned {raw_count} raw finding(s) before suppression; none of them belongs to `{name}`"
            )),
            ..base(entry, project)
        };
    }
    if entry.declared_public {
        return Finding {
            summary: format!(
                "`[reachability] public_api` entry for {} in `{}` matched nothing this run -- its path or symbol may be gone",
                target(entry),
                entry.file
            ),
            detail: format!(
                "This declaration was written because: \"{}\". No `reachability` finding this run is in a file matching `{}` (and the symbol, when given), \
                 so it declares nothing. Fix the path or symbol if it is misspelt, remove it if the API is gone, or keep it if it is \
                 written for items that are not reported today.",
                entry.reason, entry.file
            ),
            positive_control: Some(format!(
                "check `reachability` ran to completion this run and returned {raw_count} raw finding(s) before suppression; none of them matched this entry"
            )),
            ..base(entry, project)
        };
    }
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
            claim: crate::finding::Claim::Presence,
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

    #[test]
    fn a_finding_in_the_projects_own_conformance_fixture_is_left_out_and_counted() {
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("conformance/SUP-001/defective");
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::write(fixture.join("expect.toml"), "").unwrap();
        // the same shape, but no `expect.toml` beside it, and the same shape outside `conformance/`
        std::fs::create_dir_all(root.path().join("conformance/notes/draft")).unwrap();
        let al = Allowlist::empty();
        let mut s = Suppression::for_project(&al, root.path());
        let kept = s.apply(vec![
            finding(
                "SUP-001",
                "conformance/SUP-001/defective/web/package.json",
                None,
            ),
            finding("SUP-001", "conformance/notes/draft/web/package.json", None),
            finding(
                "SUP-001",
                "web/conformance/SUP-001/defective/package.json",
                None,
            ),
            finding("SUP-001", "web/package.json", None),
            finding("SUP-001", "conformance/../web/package.json", None),
        ]);
        let kept: Vec<_> = kept
            .iter()
            .map(|f| f.location.as_ref().unwrap().file.as_str())
            .collect();
        assert_eq!(
            kept,
            [
                "conformance/notes/draft/web/package.json",
                "web/conformance/SUP-001/defective/package.json",
                "web/package.json",
                "conformance/../web/package.json",
            ]
        );
        assert_eq!(s.excluded(), 1);
    }

    #[test]
    fn without_a_project_root_nothing_is_left_out() {
        let al = Allowlist::empty();
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![finding(
            "SUP-001",
            "conformance/SUP-001/defective/web/package.json",
            None,
        )]);
        assert_eq!(kept.len(), 1);
        assert_eq!(s.excluded(), 0);
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
        let _ = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        let waived = s.take_waived();
        assert_eq!(waived.len(), 1, "one finding is waived once");
        assert_eq!(waived[0].reason, "why");
    }

    #[test]
    fn a_waived_finding_is_kept_whole_with_the_entrys_reason() {
        let al = allowlist(ONE);
        let mut s = Suppression::new(&al);
        let _ = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
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
        let _ = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        s.mark_completed("a", 1);
        assert!(s.stale_findings(&["a"], "p").is_empty());
    }

    const PUBLIC: &str = "[reachability]
public_api = [
  { file = \"src/api/**\", reason = \"published API\" },
  { file = \"src/one.rs\", symbol = \"only_this\", reason = \"one item\" },
]
";

    #[test]
    fn a_declared_public_api_glob_waives_what_it_matches_and_marks_it() {
        let al = allowlist(PUBLIC);
        let mut s = Suppression::new(&al);
        let kept = s.apply(vec![
            finding("reachability", "src/api/mod.rs", Some("f")),
            finding("reachability", "src/api/deep/er.rs", Some("g")),
            finding("reachability", "src/other.rs", Some("h")),
            finding("reachability", "src/one.rs", Some("only_this")),
            finding("reachability", "src/one.rs", Some("not_this")),
            finding("unused-return-values", "src/api/mod.rs", Some("f")),
        ]);
        let kept: Vec<_> = kept.iter().map(|f| f.subject.clone().unwrap()).collect();
        assert_eq!(kept, vec!["h", "not_this", "f"]);
        let waived = s.take_waived();
        assert_eq!(waived.len(), 3);
        assert!(waived.iter().all(|w| w.declared_public), "{waived:?}");
        assert_eq!(waived[0].reason, "published API");
    }

    const PUBLIC_CRATES: &str = "[reachability]
public_api_crates = [
  { crate = \"ffi-sys\", reason = \"FFI bindings: every pub item is API\" },
]
";

    #[test]
    fn a_crate_level_declaration_covers_every_finding_of_that_package_and_no_other() {
        let al = allowlist(PUBLIC_CRATES);
        let mut s = Suppression::new(&al);
        let in_member = |file: &str, member: &str, symbol: &str| {
            let mut f = finding("reachability", file, Some(symbol));
            f.member = Some(member.into());
            f
        };
        let kept = s.apply(vec![
            // paths are relative to each member in a workspace run: the package name, not the path, picks the crate
            in_member("src/lib.rs", "ffi-sys", "a"),
            in_member("src/types.rs", "ffi-sys", "b"),
            in_member("src/lib.rs", "other", "c"),
            // another check in the declared crate is not covered
            {
                let mut f = in_member("src/lib.rs", "ffi-sys", "d");
                f.check_id = "unused-return-values".into();
                f
            },
        ]);
        let kept: Vec<_> = kept.iter().map(|f| f.subject.clone().unwrap()).collect();
        assert_eq!(kept, vec!["c", "d"]);
        let waived = s.take_waived();
        assert_eq!(waived.len(), 2);
        assert!(waived.iter().all(|w| w.declared_public), "{waived:?}");
        assert_eq!(waived[0].reason, "FFI bindings: every pub item is API");
    }

    #[test]
    fn outside_a_workspace_run_the_crate_is_matched_by_the_project_name() {
        let al = allowlist(PUBLIC_CRATES);
        let mut s = Suppression::new(&al);
        let mut hit = finding("reachability", "src/lib.rs", Some("a"));
        hit.project = "ffi-sys".into();
        let miss = finding("reachability", "src/lib.rs", Some("b"));
        let kept = s.apply(vec![hit, miss]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].subject.as_deref(), Some("b"));
    }

    #[test]
    fn a_crate_declaration_that_matches_nothing_warns_by_crate_name() {
        let al = allowlist(PUBLIC_CRATES);
        let mut s = Suppression::new(&al);
        let _ = s.apply(vec![finding("reachability", "src/lib.rs", Some("a"))]);
        s.mark_completed("reachability", 1);
        let stale = s.stale_findings(&["reachability"], "p");
        assert_eq!(stale.len(), 1);
        assert!(
            stale[0].summary.contains("public_api_crates"),
            "{}",
            stale[0].summary
        );
        assert!(stale[0].summary.contains("ffi-sys"), "{}", stale[0].summary);
    }

    #[test]
    fn an_ordinary_waiver_is_not_declared_public() {
        let al = allowlist(ONE);
        let mut s = Suppression::new(&al);
        let _ = s.apply(vec![finding("a", "src/x.rs", Some("f"))]);
        assert!(!s.take_waived()[0].declared_public);
    }

    #[test]
    fn a_public_api_entry_that_matches_nothing_warns_by_name() {
        let al = allowlist(PUBLIC);
        let mut s = Suppression::new(&al);
        let _ = s.apply(vec![finding("reachability", "src/api/mod.rs", Some("f"))]);
        s.mark_completed("reachability", 1);
        let stale = s.stale_findings(&["reachability"], "p");
        assert_eq!(stale.len(), 1, "{stale:?}");
        assert!(
            stale[0].summary.contains("public_api"),
            "{}",
            stale[0].summary
        );
        assert!(
            stale[0].summary.contains("src/one.rs"),
            "{}",
            stale[0].summary
        );
    }

    #[test]
    fn the_glob_star_stays_inside_a_directory_and_double_star_crosses_them() {
        for (pattern, path, want) in [
            ("src/*.rs", "src/a.rs", true),
            ("src/*.rs", "src/sub/a.rs", false),
            ("src/**", "src/sub/a.rs", true),
            ("src/**/a.rs", "src/a.rs", true),
            ("src/**/a.rs", "src/x/y/a.rs", true),
            ("src/a?.rs", "src/ab.rs", true),
            ("src/a?.rs", "src/a.rs", false),
            ("src/api.rs", "src/api_v2.rs", false),
        ] {
            assert_eq!(glob_match(pattern, path), want, "{pattern} vs {path}");
        }
    }
}
