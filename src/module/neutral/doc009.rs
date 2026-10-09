//! DOC-009: the project keeps a glossary, and defines each term once.
//!
//! **What this judges:** a repository with documentation (at least one tracked `.md` file under `docs/` or `doc/` at the root) must track a
//! glossary: a file whose name contains `glossary` (any case, any directory). In each glossary, a term (a heading of level two
//! or deeper, or a line that starts with a bold term) defined twice is reported: two definitions of one word is the deterministic
//! shape of an inconsistent vocabulary.
//!
//! **What it cannot see:** the other half of the rule, that documents and code *use* the glossary's terms the same way, needs
//! language understanding and is not judged (the rule's own confidence is low). A repository without a `docs/` directory is
//! `not applicable`; so is one whose documentation sits elsewhere (`packages/x/docs`). A line that starts with a bold word counts as a
//! term even when it is not one (`**Note:**`), and a `##` line inside a code fence is read as a heading.

use super::{file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = "DOC-009";

fn is_glossary(path: &str) -> bool {
    file_name(path).to_lowercase().contains("glossary")
}

fn is_documentation(path: &str) -> bool {
    path.ends_with(".md") && (path.starts_with("docs/") || path.starts_with("doc/"))
}

/// The term a glossary line defines, lower-cased: a heading of level two or deeper, or a line that starts with a bold term
/// (optionally as a list item).
fn defined_term(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(rest) = line.strip_prefix("##") {
        let heading = rest.trim_start_matches('#').trim();
        return (!heading.is_empty()).then(|| heading.to_lowercase());
    }
    let bold = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .unwrap_or(line)
        .strip_prefix("**")?;
    let (term, _) = bold.split_once("**")?;
    let term = term.trim().trim_end_matches(':').trim();
    (!term.is_empty()).then(|| term.to_lowercase())
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    if !view.paths().iter().any(|p| is_documentation(p)) {
        return Verdict::NotApplicable(
            "the repository tracks no documentation directory (docs/ or doc/) that a glossary would serve".into(),
        );
    }
    let glossaries: Vec<&String> = view.paths().iter().filter(|p| is_glossary(p)).collect();
    if glossaries.is_empty() {
        let docs = view.paths().iter().filter(|p| is_documentation(p)).count();
        return Verdict::Findings(vec![Finding::new(
            ID,
            Severity::Info,
            Confidence::Low,
            view.name(),
            "the repository documents itself but keeps no glossary",
            "A shared vocabulary lowers misreading. Add a GLOSSARY.md (or docs/glossary.md) that defines the project's domain terms.",
        )
        .location(Location::new("GLOSSARY.md", None))
        .subject("glossary")
        .positive_control(format!(
            "{docs} documentation file(s) are tracked under docs/ or doc/, and none of the {} tracked paths has a file name containing `glossary`",
            view.paths().len()
        ))]);
    }
    let mut findings = Vec::new();
    for path in glossaries {
        let Some(text) = view.read(path) else {
            continue;
        };
        let mut seen: Vec<String> = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let Some(term) = defined_term(line) else {
                continue;
            };
            if seen.contains(&term) {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Info,
                        Confidence::Low,
                        view.name(),
                        format!("{path} defines the term `{term}` more than once"),
                        "One word with two definitions is a source of misreading. Merge the definitions, or rename one of the terms.",
                    )
                    .location(Location::new(path.as_str(), u32::try_from(index + 1).ok()))
                    .positive_control(format!(
                        "{path} was read; `{term}` was already defined earlier in it"
                    ))
                    .subject(term),
                );
            } else {
                seen.push(term);
            }
        }
    }
    Verdict::Findings(findings)
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
    fn documentation_without_a_glossary_is_reported() {
        let found = run(&[("docs/guide.md", "# Guide\n"), ("README.md", "hi")]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].check_id, "DOC-009");
        assert_eq!(found[0].subject.as_deref(), Some("glossary"));
        assert!(found[0].clone().validate().is_ok());
    }

    #[test]
    fn a_glossary_anywhere_in_any_case_satisfies_the_rule() {
        for path in ["GLOSSARY.md", "docs/glossary.md", "doc/Project-Glossary.md"] {
            let found = run(&[
                ("docs/guide.md", "# Guide\n"),
                (path, "## Term\n\nmeans x\n"),
            ]);
            assert!(found.is_empty(), "{path}: {found:?}");
        }
    }

    #[test]
    fn a_term_defined_twice_is_reported_at_its_second_definition() {
        let glossary = "# Glossary\n\n## Rule\n\nA check.\n\n## Module\n\nA unit.\n\n## rule\n\nA law.\n\n**Module** - again\n";
        let found = run(&[("docs/guide.md", "g"), ("docs/glossary.md", glossary)]);
        let mut lines: Vec<(Option<String>, Option<u32>)> = found
            .iter()
            .map(|f| (f.subject.clone(), f.location.as_ref().unwrap().line))
            .collect();
        lines.sort();
        assert_eq!(
            lines,
            vec![
                (Some("module".into()), Some(15)),
                (Some("rule".into()), Some(11))
            ]
        );
    }

    #[test]
    fn a_repository_without_documentation_is_not_applicable() {
        let view = RepoView::from_files(&[("README.md", "x"), ("src/lib.rs", "")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
    }

    #[test]
    fn distinct_terms_are_clean() {
        let found = run(&[
            ("docs/guide.md", "g"),
            (
                "GLOSSARY.md",
                "# Glossary\n\n## Alpha\n\n## Beta\n\n- **Gamma**: c\n",
            ),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }
}
