// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-002 reviewed waivers: a visible, CodeRipper-side list of verified false positives of the secret scan.
//!
//! A repository cannot silence its own secret scan (`.gitleaksignore` is ignored on purpose), so a false positive in history that
//! can never change needs a waiver from outside it. The list is `waivers/secrets.toml` in **this** repository, shipped inside the
//! binary, edited only through a reviewed PR (design: `docs/secret-waivers-design.md`).
//!
//! **Matching is exact:** an entry applies to a leak iff `repo`, `commit`, `file`, `rule` and `line` all equal the leak's. No glob,
//! no prefix, never a rule or a path: a real secret pasted on the next line, or in the next commit, is still reported.
//! **Waived is not dropped:** the leak becomes an `Info` finding that says who reviewed it and why. **An entry that stops being true
//! fails:** past `expires`, the leak returns as `Medium` ("waiver expired"); an entry whose commit is in the scanned history but
//! that matches no leak any more is `Medium` ("waiver matches nothing"), so the list cannot silently rot.

use crate::architecture::Date;
use serde::Deserialize;

const LIST: &str = include_str!("../../../waivers/secrets.toml");
/// The longest a waiver may run after its review, in days (a renewal is a new review, never "no expiry").
const MAX_DAYS: i64 = 365;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Waiver {
    /// GitHub `owner/name` of the scanned repository.
    pub repo: String,
    /// Full 40-hex SHA of the commit the leak is in.
    pub commit: String,
    pub file: String,
    /// The gitleaks `RuleID`.
    pub rule: String,
    pub line: u32,
    pub reviewer: String,
    pub reviewed: String,
    pub expires: String,
    pub evidence: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct List {
    #[serde(default)]
    waiver: Vec<Waiver>,
}

/// What the waivers need to know of a reported finding. The scan's own record implements it, so this module does not depend on the scan
/// (the two would otherwise be a cycle between modules).
pub(super) trait Reported {
    fn commit(&self) -> &str;
    /// The file as gitleaks names it, from the repository root, with `/` separators.
    fn file(&self) -> String;
    fn rule(&self) -> &str;
    fn line(&self) -> u32;
}

impl Waiver {
    fn names(&self, leak: &impl Reported) -> bool {
        self.commit == leak.commit()
            && self.file == leak.file()
            && self.rule == leak.rule()
            && self.line == leak.line()
    }

    pub(super) fn fingerprint(&self) -> String {
        format!("{}:{}:{}:{}", self.commit, self.file, self.rule, self.line)
    }

    /// `reviewed by pm 2026-11-17, expires 2027-02-15: <evidence>`
    pub(super) fn reason(&self) -> String {
        format!(
            "reviewed by {} {}, expires {}: {}",
            self.reviewer, self.reviewed, self.expires, self.evidence
        )
    }
}

/// What the waivers did to one scan's leaks.
#[derive(Debug)]
pub(super) struct Applied<L> {
    /// Leaks no waiver names: reported as they are.
    pub open: Vec<L>,
    /// Leaks a live waiver names: reported as `Info`.
    pub waived: Vec<(L, Waiver)>,
    /// Leaks an expired waiver names: reported as `Medium`.
    pub expired: Vec<(L, Waiver)>,
    /// Live waivers of this repository whose commit is in the history but which match no leak.
    pub stale: Vec<Waiver>,
}

/// Reads the embedded list; an error names the entry. Run by a unit test, so a bad list never ships.
pub(super) fn load() -> Result<Vec<Waiver>, String> {
    parse(LIST)
}

fn parse(text: &str) -> Result<Vec<Waiver>, String> {
    let list: List = toml::from_str(text).map_err(|e| format!("waivers/secrets.toml: {e}"))?;
    validate(&list.waiver)?;
    Ok(list.waiver)
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days(d: Date) -> i64 {
    let y = i64::from(d.year) - i64::from(d.month <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(d.month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn validate(waivers: &[Waiver]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for w in waivers {
        let at = format!("waiver {}: {}", w.repo, w.fingerprint());
        let full_sha = w.commit.len() == 40
            && w.commit
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !full_sha {
            return Err(format!("{at}: `commit` is not a full lowercase 40-hex SHA"));
        }
        if w.repo.split('/').count() != 2 || w.repo.split('/').any(str::is_empty) {
            return Err(format!("{at}: `repo` is not `owner/name`"));
        }
        for (what, value) in [
            ("file", &w.file),
            ("rule", &w.rule),
            ("evidence", &w.evidence),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{at}: `{what}` is empty"));
            }
        }
        if w.line == 0 {
            return Err(format!("{at}: `line` must be 1 or more"));
        }
        if w.reviewer != "pm" {
            return Err(format!(
                "{at}: `reviewer` must be \"pm\" (a lane never waives a finding)"
            ));
        }
        let (Some(reviewed), Some(expires)) = (Date::parse(&w.reviewed), Date::parse(&w.expires))
        else {
            return Err(format!(
                "{at}: `reviewed` and `expires` must be YYYY-MM-DD dates"
            ));
        };
        let span = days(expires) - days(reviewed);
        if span <= 0 || span > MAX_DAYS {
            return Err(format!(
                "{at}: `expires` must be after `reviewed` and at most {MAX_DAYS} days later (it is {span})"
            ));
        }
        if !seen.insert((w.repo.to_ascii_lowercase(), w.fingerprint())) {
            return Err(format!("{at}: listed twice"));
        }
    }
    Ok(())
}

/// Applies `waivers` to the leaks of the repository `repo` (`None`: `origin` unknown, so nothing applies). `in_history` says
/// whether a commit SHA is in the scanned history (a waiver for a commit that is not there applies to nothing, and is not stale).
pub(super) fn apply<L: Reported>(
    waivers: &[Waiver],
    repo: Option<&str>,
    leaks: Vec<L>,
    today: Date,
    in_history: impl Fn(&str) -> bool,
) -> Applied<L> {
    let mut out = Applied {
        open: Vec::new(),
        waived: Vec::new(),
        expired: Vec::new(),
        stale: Vec::new(),
    };
    let Some(repo) = repo else {
        out.open = leaks;
        return out;
    };
    let mine: Vec<&Waiver> = waivers
        .iter()
        .filter(|w| w.repo.eq_ignore_ascii_case(repo))
        .collect();
    let mut used = vec![false; mine.len()];
    for leak in leaks {
        match mine.iter().position(|w| w.names(&leak)) {
            None => out.open.push(leak),
            Some(i) => {
                used[i] = true;
                let w = mine[i].clone();
                // `validate` ran on the list, so the date parses; a list that was not validated never waives
                let live = Date::parse(&w.expires).is_some_and(|e| today < e);
                if live {
                    out.waived.push((leak, w));
                } else {
                    out.expired.push((leak, w));
                }
            }
        }
    }
    for (w, used) in mine.iter().zip(used) {
        let live = Date::parse(&w.expires).is_some_and(|e| today < e);
        if !used && live && in_history(&w.commit) {
            out.stale.push((*w).clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRETANT: &str = "ThinkersJournal/secretant";
    const C1: &str = "6e944684ca91408834c7f0d8b8d317cd2076e35e";

    struct Leak {
        commit: String,
        file: String,
        rule: String,
        line: u32,
    }

    impl Reported for Leak {
        fn commit(&self) -> &str {
            &self.commit
        }
        fn file(&self) -> String {
            self.file.replace('\\', "/")
        }
        fn rule(&self) -> &str {
            &self.rule
        }
        fn line(&self) -> u32 {
            self.line
        }
    }

    fn leak(commit: &str, file: &str, rule: &str, line: u32) -> Leak {
        Leak {
            commit: commit.into(),
            file: file.into(),
            rule: rule.into(),
            line,
        }
    }

    fn date(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn list() -> Vec<Waiver> {
        load().expect("the embedded list is valid")
    }

    #[test]
    fn the_embedded_list_is_valid_and_holds_the_two_secretant_fingerprints() {
        let prints: Vec<String> = list().iter().map(Waiver::fingerprint).collect();
        assert_eq!(
            prints,
            [
                "6e944684ca91408834c7f0d8b8d317cd2076e35e:CHANGELOG.md:generic-api-key:5",
                "f1cc76b01a0e9afd3f1d00c95ba3dd3602fff4a1:CHANGELOG.md:generic-api-key:5",
            ]
        );
    }

    #[test]
    fn an_exact_match_is_waived_and_any_one_changed_field_is_not() {
        let l = list();
        let today = date("2026-11-18");
        let a = apply(
            &l,
            Some(SECRETANT),
            vec![leak(C1, "CHANGELOG.md", "generic-api-key", 5)],
            today,
            |_| true,
        );
        assert_eq!((a.waived.len(), a.open.len(), a.expired.len()), (1, 0, 0));
        for changed in [
            leak(
                "0000000000000000000000000000000000000000",
                "CHANGELOG.md",
                "generic-api-key",
                5,
            ),
            leak(C1, "README.md", "generic-api-key", 5),
            leak(C1, "CHANGELOG.md", "aws-access-token", 5),
            // a real secret pasted on the next line is still reported
            leak(C1, "CHANGELOG.md", "generic-api-key", 6),
        ] {
            let a = apply(&l, Some(SECRETANT), vec![changed], today, |_| true);
            assert_eq!((a.waived.len(), a.open.len()), (0, 1));
        }
        // another repository, and an unknown origin, get nothing
        for repo in [Some("someone/else"), None] {
            let a = apply(
                &l,
                repo,
                vec![leak(C1, "CHANGELOG.md", "generic-api-key", 5)],
                today,
                |_| true,
            );
            assert_eq!((a.waived.len(), a.open.len()), (0, 1), "{repo:?}");
        }
    }

    #[test]
    fn the_repository_name_is_compared_without_case() {
        let a = apply(
            &list(),
            Some("thinkersjournal/SecretAnt"),
            vec![leak(C1, "CHANGELOG.md", "generic-api-key", 5)],
            date("2026-11-18"),
            |_| true,
        );
        assert_eq!(a.waived.len(), 1);
    }

    #[test]
    fn an_expired_entry_returns_the_leak_and_is_not_also_stale() {
        let a = apply(
            &list(),
            Some(SECRETANT),
            vec![leak(C1, "CHANGELOG.md", "generic-api-key", 5)],
            date("2027-02-15"),
            |_| true,
        );
        assert_eq!((a.waived.len(), a.expired.len(), a.open.len()), (0, 1, 0));
        // 2027-02-15 is the expiry day itself: the entry has ended
        assert!(a.stale.is_empty());
    }

    #[test]
    fn an_entry_that_matches_nothing_is_stale_only_when_its_commit_is_in_the_history() {
        let l = list();
        let today = date("2026-11-18");
        let present = apply(&l, Some(SECRETANT), Vec::<Leak>::new(), today, |_| true);
        assert_eq!(present.stale.len(), 2);
        let shallow = apply(&l, Some(SECRETANT), Vec::<Leak>::new(), today, |_| false);
        assert!(
            shallow.stale.is_empty(),
            "a commit that is not in the history applies to nothing"
        );
        let elsewhere = apply(&l, Some("someone/else"), Vec::<Leak>::new(), today, |_| {
            true
        });
        assert!(elsewhere.stale.is_empty());
    }

    fn one(edit: impl Fn(&str) -> String) -> Result<Vec<Waiver>, String> {
        let base = r#"[[waiver]]
repo = "o/n"
commit = "6e944684ca91408834c7f0d8b8d317cd2076e35e"
file = "a.md"
rule = "generic-api-key"
line = 5
reviewer = "pm"
reviewed = "2026-11-17"
expires = "2027-02-15"
evidence = "prose"
"#;
        assert!(parse(base).is_ok(), "the control entry is valid");
        parse(&edit(base))
    }

    #[test]
    fn a_malformed_list_is_an_error_not_a_skip() {
        let cases: [(&str, &str, &str); 8] = [
            (
                "short sha",
                "6e944684ca91408834c7f0d8b8d317cd2076e35e",
                "6e94468",
            ),
            (
                "upper sha",
                "6e944684ca91408834c7f0d8b8d317cd2076e35e",
                "6E944684CA91408834C7F0D8B8D317CD2076E35E",
            ),
            (
                "lane reviewer",
                "reviewer = \"pm\"",
                "reviewer = \"coderipper\"",
            ),
            (
                "expires before reviewed",
                "expires = \"2027-02-15\"",
                "expires = \"2026-11-16\"",
            ),
            (
                "past the 365-day cap",
                "expires = \"2027-02-15\"",
                "expires = \"2027-11-18\"",
            ),
            (
                "not a date",
                "expires = \"2027-02-15\"",
                "expires = \"2027-02-30\"",
            ),
            (
                "empty evidence",
                "evidence = \"prose\"",
                "evidence = \"  \"",
            ),
            ("zero line", "line = 5", "line = 0"),
        ];
        for (what, from, to) in cases {
            assert!(one(|b| b.replace(from, to)).is_err(), "{what}");
        }
        assert!(
            one(|b| b.replace("evidence = \"prose\"", "evidence = \"p\"\nextra = 1")).is_err(),
            "unknown field"
        );
        assert!(
            one(|b| b.replace("evidence = \"prose\"\n", "")).is_err(),
            "missing field"
        );
        assert!(
            one(|b| format!("{b}\n{b}")).is_err(),
            "duplicate fingerprint"
        );
        // exactly 365 days is allowed
        assert!(one(|b| b.replace("expires = \"2027-02-15\"", "expires = \"2027-11-17\"")).is_ok());
    }
}
