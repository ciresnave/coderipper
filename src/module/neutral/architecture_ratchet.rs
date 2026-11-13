// SPDX-License-Identifier: MIT OR Apache-2.0
//! The architecture-waiver ratchet: once a repository's `[architecture]` declaration says `ratchet = true`, the number of
//! `[[architecture.waiver]]` entries may only fall.
//!
//! Rule record: `architecture-ratchet` in `rules/gov.toml` (related `GOV-001`, the "ratchet downward" idea, and `ARC-001`, which reports
//! an expired or unused waiver). The choice that makes it opt-in is the sixth of the 2026-11-01 declaration proposal, accepted by CireSnave
//! (ruling 365): *"the count of waivers may only fall (Fuel's rule for its allowlist) once a project opts in with `ratchet = true`"*.
//!
//! **What it judges.** Only a repository whose `.coderipper.toml` at `HEAD` has an `[architecture]` table with `ratchet = true`. It counts the
//! `[[architecture.waiver]]` entries of that declaration at `HEAD` and at the commit the branch started from (the merge-base, found as
//! `lint-suppression` and `version-and-changelog` find it), both read with the declaration's own parser. More waivers now than at the base is
//! a finding at the first waiver that is a surplus one (the first beyond the base's count, in file order); a waiver that was replaced by
//! another keeps the count and is not a finding. Reading the count, not the identity of each waiver, is deliberate: an exception that is
//! swapped for another says so in its own `reason`, `owner` and end date, and ARC-001 reads those.
//!
//! **Never clean when it did not run.** No base found (no default branch, a clone without history) is a note, "ratchet not run": a gap.
//! A run on the default branch has no branch to compare, "not applicable". A base with no `[architecture]` table has nothing to ratchet
//! against (the branch introduces the declaration), "not applicable". A repository with no declaration, or with `ratchet` absent or false,
//! is "not applicable": the ratchet is opt-in. A `.coderipper.toml` that cannot be parsed, at `HEAD` or at the base, is a note, never a pass.
//!
//! **Advisory.** It is not in `portfolio_floor`, so the portfolio profile reports it and never fails on it; Medium severity, High confidence
//! (the counts are exact).
//!
//! **Not judged:** `[[architecture.allow]]` edges (they carry a stated reason and are part of the declared design, not an exception to it),
//! a declaration moved to another file (`[architecture] file = ...` is not read by the loader either), and waivers of other tools
//! (`[[allow]]` entries, lint suppressions: `lint-suppression` ratchets those).

use super::{Base, RepoView, Verdict};
use crate::config::{ConfigFile, FILE_NAME};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::path::Path;

const ID: &str = crate::rule_id::ARCHITECTURE_RATCHET;

/// What one `.coderipper.toml` says about the ratchet.
struct Declared {
    /// `ratchet = true`.
    ratchet: bool,
    /// How many `[[architecture.waiver]]` entries.
    waivers: usize,
}

/// The declaration in a `.coderipper.toml` text: `Ok(None)` when it declares no `[architecture]` table, `Err` when it cannot be parsed.
fn declared(text: &str) -> Result<Option<Declared>, String> {
    let file = ConfigFile::parse(text, Path::new(FILE_NAME)).map_err(|e| e.to_string())?;
    Ok(file.architecture.map(|a| Declared {
        ratchet: a.ratchet == Some(true),
        waivers: a.waiver.len(),
    }))
}

/// The 1-based line of the `index`-th (0-based) `[[architecture.waiver]]` header of a file, or line 1.
fn waiver_line(text: &str, index: usize) -> u32 {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim().starts_with("[[architecture.waiver]]"))
        .nth(index)
        .and_then(|(i, _)| u32::try_from(i + 1).ok())
        .unwrap_or(1)
}

fn not_run(why: String) -> Verdict {
    Verdict::Noted(Vec::new(), vec![format!("{ID}: ratchet not run: {why}")])
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let Some(text) = view.read(FILE_NAME) else {
        return Verdict::NotApplicable(format!(
            "the repository tracks no {FILE_NAME}, so it declares no [architecture] and no ratchet"
        ));
    };
    let head = match declared(&text) {
        Ok(Some(d)) => d,
        Ok(None) => return Verdict::NotApplicable(
            "the repository declares no [architecture] table, so there are no waivers to ratchet"
                .into(),
        ),
        Err(e) => return not_run(format!("{FILE_NAME} could not be parsed at HEAD: {e}")),
    };
    if !head.ratchet {
        return Verdict::NotApplicable(
            "[architecture] does not say ratchet = true: the waiver ratchet is opt-in".into(),
        );
    }
    let base = match view.base() {
        Base::Tree(base) => base,
        Base::SameAsHead(why) => return Verdict::NotApplicable(why),
        Base::Gap(why) => return not_run(why),
    };
    let then = match base.read(FILE_NAME) {
        None => {
            return Verdict::NotApplicable(format!(
                "{FILE_NAME} did not exist at {}: the branch introduces the declaration, so there is nothing to ratchet against",
                base.how()
            ))
        }
        Some(base_text) => match declared(&base_text) {
            Ok(Some(d)) => d,
            Ok(None) => {
                return Verdict::NotApplicable(format!(
                    "{FILE_NAME} had no [architecture] table at {}: the branch introduces the declaration, so there is nothing to ratchet against",
                    base.how()
                ))
            }
            Err(e) => {
                return not_run(format!(
                    "{FILE_NAME} could not be parsed at {}: {e}",
                    base.how()
                ))
            }
        },
    };
    let note = format!(
        "{ID}: {} waiver(s) at HEAD, {} at {}",
        head.waivers,
        then.waivers,
        base.how()
    );
    if head.waivers <= then.waivers {
        return Verdict::Noted(Vec::new(), vec![note]);
    }
    let line = waiver_line(&text, then.waivers);
    let finding = Finding::new(
        ID,
        Severity::Medium,
        Confidence::High,
        view.name(),
        format!(
            "the architecture declaration has {} waiver(s), {} at {}, and it says ratchet = true",
            head.waivers,
            then.waivers,
            base.how()
        ),
        "With `ratchet = true` the number of `[[architecture.waiver]]` entries may only fall: every exception is meant to be paid back, not to \
         accumulate. This branch adds a waiver (the first surplus one is at this line). Fix the violation instead, or remove another waiver in \
         the same change, or set `ratchet = false` and say why in the review. A waiver that replaces another keeps the count and is not reported."
            .to_string(),
    )
    .location(Location::new(FILE_NAME, Some(line)))
    .subject("architecture-waiver-count".to_string())
    .positive_control(format!(
        "{FILE_NAME} was parsed at HEAD and at {}: {} waiver(s) now and {} before",
        base.how(),
        head.waivers,
        then.waivers
    ));
    Verdict::Noted(vec![finding], vec![note])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(ratchet: Option<bool>, waivers: usize) -> String {
        let mut s = String::from("[architecture]\n");
        if let Some(r) = ratchet {
            s.push_str(&format!("ratchet = {r}\n"));
        }
        s.push_str("[[architecture.component]]\nname = \"a\"\npaths = [\"a\"]\n");
        for i in 0..waivers {
            s.push_str(&format!(
                "[[architecture.waiver]]\nedge = [\"a\", \"b{i}\"]\nreason = \"r\"\nowner = \"o\"\nuntil = \"2027-01-01\"\n"
            ));
        }
        s
    }

    fn run(head: &str, base: Option<&str>) -> Verdict {
        let base_files: Option<Vec<(&str, &str)>> = base.map(|b| vec![(FILE_NAME, b)]);
        judge(&RepoView::from_files_with_base(
            &[(FILE_NAME, head)],
            base_files.as_deref(),
        ))
    }

    fn findings(v: Verdict) -> Vec<Finding> {
        match v {
            Verdict::Noted(f, _) | Verdict::Findings(f) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn notes(v: &Verdict) -> Vec<String> {
        match v {
            Verdict::Noted(_, n) => n.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn more_waivers_than_the_base_with_the_ratchet_on_is_a_finding_at_the_first_surplus_one() {
        let f = findings(run(&decl(Some(true), 3), Some(&decl(Some(true), 2))));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Medium);
        let at = f[0].location.as_ref().unwrap();
        assert_eq!(at.file, FILE_NAME);
        // the third `[[architecture.waiver]]` header: the file's own lines are counted
        let text = decl(Some(true), 3);
        assert_eq!(at.line, Some(waiver_line(&text, 2)));
        assert!(f[0].summary.contains("3 waiver(s)"), "{}", f[0].summary);
    }

    #[test]
    fn the_same_count_or_fewer_is_not_a_finding_and_says_so() {
        let same = run(&decl(Some(true), 2), Some(&decl(Some(true), 2)));
        assert!(
            notes(&same)[0].contains("2 waiver(s) at HEAD, 2 at"),
            "{:?}",
            notes(&same)
        );
        assert!(findings(same).is_empty());
        assert!(findings(run(&decl(Some(true), 1), Some(&decl(Some(true), 2)))).is_empty());
    }

    #[test]
    fn the_ratchet_is_opt_in_and_a_declaration_is_needed() {
        for head in [decl(None, 3), decl(Some(false), 3)] {
            assert!(matches!(
                run(&head, Some(&decl(Some(true), 0))),
                Verdict::NotApplicable(_)
            ));
        }
        let none = judge(&RepoView::from_files(&[("src/lib.rs", "")]));
        assert!(matches!(none, Verdict::NotApplicable(_)));
        let no_table = run(
            "[[allow]]\ncheck = \"x\"\nfile = \"f\"\nreason = \"r\"\n",
            None,
        );
        assert!(matches!(no_table, Verdict::NotApplicable(_)));
    }

    #[test]
    fn no_base_is_a_gap_not_a_pass_and_a_base_without_a_declaration_is_not_applicable() {
        let gap = run(&decl(Some(true), 3), None);
        assert!(
            notes(&gap)[0].contains("ratchet not run"),
            "{:?}",
            notes(&gap)
        );
        assert!(findings(gap).is_empty());
        let introduced = run(
            &decl(Some(true), 3),
            Some("[[allow]]\ncheck = \"x\"\nfile = \"f\"\nreason = \"r\"\n"),
        );
        assert!(matches!(introduced, Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_file_that_cannot_be_parsed_is_a_note_never_a_pass() {
        let bad_head = run("[architecture\nratchet = true", Some(&decl(Some(true), 0)));
        assert!(
            notes(&bad_head)[0].contains("could not be parsed at HEAD"),
            "{:?}",
            notes(&bad_head)
        );
        let bad_base = run(&decl(Some(true), 1), Some("[architecture\n"));
        assert!(
            notes(&bad_base)[0].contains("could not be parsed"),
            "{:?}",
            notes(&bad_base)
        );
    }

    #[test]
    fn the_line_of_the_nth_waiver_header_is_found() {
        let text = decl(Some(true), 3);
        let lines: Vec<&str> = text.lines().collect();
        let headers: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.starts_with("[[architecture.waiver]]"))
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(waiver_line(&text, 0) as usize, headers[0]);
        assert_eq!(waiver_line(&text, 2) as usize, headers[2]);
        assert_eq!(waiver_line(&text, 9), 1);
    }
}
