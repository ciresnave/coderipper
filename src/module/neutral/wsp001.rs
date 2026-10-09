//! WSP-001: every repository has a named owner.
//!
//! **What this judges (the owner part of the rule, from the repository alone):** the repository tracks a `CODEOWNERS` file
//! (at the root, in `.github/` or in `docs/`) with a default entry, a pattern of `*` followed by at least one owner (`@user`,
//! `@org/team` or an e-mail address). Comments and blank lines are ignored.
//!
//! **What it cannot see:** the rest of the rule, a portfolio inventory that also records each repository's importance and stage
//! of life, needs a place to keep it that CodeRipper does not define yet; this check says nothing about it. Whether the owner
//! named exists is not checked (that needs the forge).

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = "WSP-001";

/// Where GitHub looks for the file, in the order it looks.
const PLACES: &[&str] = &[".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

fn is_owner(token: &str) -> bool {
    token.starts_with('@') || (token.contains('@') && token.contains('.'))
}

/// `(entries, has a default owner)` for a CODEOWNERS text.
fn read_entries(text: &str) -> (usize, bool) {
    let mut entries = 0;
    let mut default_owner = false;
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        entries += 1;
        let mut tokens = line.split_whitespace();
        if tokens.next() == Some("*") && tokens.any(is_owner) {
            default_owner = true;
        }
    }
    (entries, default_owner)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let project = view.name();
    let Some(path) = PLACES.iter().find(|p| view.has(p)) else {
        return Verdict::Findings(vec![Finding::new(
            ID,
            Severity::Medium,
            Confidence::High,
            project,
            "the repository has no CODEOWNERS file, so it names no owner",
            "Nobody is recorded as answering for this repository. Add a CODEOWNERS file (.github/CODEOWNERS) with a default entry: `* @owner-or-team`.",
        )
        .location(Location::new(PLACES[0], None))
        .subject("CODEOWNERS")
        .positive_control(format!(
            "the search covered {} among {} tracked files",
            PLACES.join(", "),
            view.paths().len()
        ))]);
    };
    let Some(text) = view.read(path) else {
        return Verdict::Findings(Vec::new());
    };
    let (entries, default_owner) = read_entries(&text);
    if default_owner {
        return Verdict::Findings(Vec::new());
    }
    Verdict::Findings(vec![Finding::new(
        ID,
        Severity::Medium,
        Confidence::High,
        project,
        format!("{path} has no default owner (a `*` entry with an owner)"),
        "Files that no other pattern matches belong to nobody. Add a default entry: `* @owner-or-team`.",
    )
    .location(Location::new(path.to_string(), None))
    .subject("CODEOWNERS")
    .positive_control(format!(
        "{path} is tracked and was read; it holds {entries} ownership entr{}, none of them a `*` pattern with an owner",
        if entries == 1 { "y" } else { "ies" }
    ))])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    #[test]
    fn a_repository_without_codeowners_is_reported() {
        let found = run(&[("README.md", "x")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].check_id, "WSP-001");
        assert_eq!(
            found[0].location.as_ref().unwrap().file,
            ".github/CODEOWNERS"
        );
        assert_eq!(found[0].subject.as_deref(), Some("CODEOWNERS"));
        assert!(found[0].clone().validate().is_ok());
    }

    #[test]
    fn a_default_owner_in_any_of_the_three_places_is_clean() {
        for path in ["CODEOWNERS", ".github/CODEOWNERS", "docs/CODEOWNERS"] {
            let found = run(&[(path, "# owners\n\n* @org/team\n")]);
            assert!(found.is_empty(), "{path}: {found:?}");
        }
        assert!(run(&[("CODEOWNERS", "*   dev@example.com\n")]).is_empty());
    }

    #[test]
    fn a_file_with_no_default_entry_is_reported_at_the_file() {
        let found = run(&[(".github/CODEOWNERS", "/src/ @alice\n*.md @bob\n")]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].location.as_ref().unwrap().file,
            ".github/CODEOWNERS"
        );
    }

    #[test]
    fn a_default_pattern_with_no_owner_does_not_count() {
        assert_eq!(run(&[("CODEOWNERS", "*\n")]).len(), 1);
        assert_eq!(run(&[("CODEOWNERS", "* not-an-owner\n")]).len(), 1);
        assert_eq!(run(&[("CODEOWNERS", "# * @commented-out\n")]).len(), 1);
    }

    #[test]
    fn the_first_file_in_search_order_is_the_one_read() {
        // GitHub reads .github/, then the root, then docs/; so does this check.
        let found = run(&[
            (".github/CODEOWNERS", "/src/ @a\n"),
            ("CODEOWNERS", "* @b\n"),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
