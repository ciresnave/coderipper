// SPDX-License-Identifier: MIT OR Apache-2.0
//! DOC-003: each declared component carries a purpose and an owner.
//!
//! **What this judges:** the `[[architecture.component]]` entries of the repository's `.coderipper.toml` at `HEAD`, read with the
//! declaration's own parser. Two optional keys on a component, added with this rule (PM ruling, 2026-11-15: additive, absent means the rule
//! is silent): `purpose = "what it is for"` and `owner = "who answers for it"`.
//!
//! **Adoption is by presence.** A declaration in which *no* component carries a `purpose` or an `owner` has not adopted the convention
//! and gets "not applicable", never a finding per component (it would be one finding for every component of every repository that has an
//! `[architecture]` table, none of which was asked). Once any component carries either key, **every** component must carry both: one
//! **Low**-severity, High-confidence finding per component that lacks a key or whose value is blank (empty or only spaces), at the
//! component's `[[architecture.component]]` header, naming what is missing.
//!
//! **What it cannot see:** whether a purpose says anything (`"stuff"` passes), whether the owner is a real person or lane, a declaration
//! moved to another file (`[architecture] file = ...` is not read by the loader either), and purposes written only as comments in the
//! TOML. A `.coderipper.toml` that cannot be parsed is a note, never a pass. Opt-in: it runs when named (`coderipper check DOC-003`).

use super::{RepoView, Verdict};
use crate::config::{ConfigFile, FILE_NAME};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::path::Path;

const ID: &str = crate::rule_id::DOC_003;

/// The 1-based line of the `index`-th (0-based) `[[architecture.component]]` header of a file, or line 1.
fn header_line(text: &str, index: usize) -> u32 {
    text.lines()
        .enumerate()
        .filter(|(_, l)| l.trim().starts_with("[[architecture.component]]"))
        .nth(index)
        .and_then(|(i, _)| u32::try_from(i + 1).ok())
        .unwrap_or(1)
}

fn blank(v: &Option<String>) -> bool {
    v.as_deref().is_none_or(|s| s.trim().is_empty())
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let Some(text) = view.read(FILE_NAME) else {
        return Verdict::NotApplicable(format!(
            "the repository tracks no {FILE_NAME}, so it declares no components"
        ));
    };
    let file = match ConfigFile::parse(&text, Path::new(FILE_NAME)) {
        Ok(f) => f,
        Err(e) => {
            return Verdict::Noted(
                Vec::new(),
                vec![format!(
                    "{ID}: not run: {FILE_NAME} could not be parsed: {e}"
                )],
            )
        }
    };
    let Some(arch) = file.architecture else {
        return Verdict::NotApplicable(
            "the repository declares no [architecture] table, so it declares no components".into(),
        );
    };
    if arch.component.is_empty() {
        return Verdict::NotApplicable("[architecture] declares no component".into());
    }
    let adopted = arch
        .component
        .iter()
        .any(|c| c.purpose.is_some() || c.owner.is_some());
    if !adopted {
        return Verdict::NotApplicable(
            "no component carries a purpose or an owner: the convention is opt-in, adopted by writing either key on one component".into(),
        );
    }
    let total = arch.component.len();
    let mut findings = Vec::new();
    for (i, c) in arch.component.iter().enumerate() {
        let mut missing = Vec::new();
        if blank(&c.purpose) {
            missing.push("purpose");
        }
        if blank(&c.owner) {
            missing.push("owner");
        }
        if missing.is_empty() {
            continue;
        }
        findings.push(
            Finding::new(
                ID,
                Severity::Low,
                Confidence::High,
                view.name(),
                format!(
                    "the component `{}` has no {} (blank or absent), while other components of the declaration carry them",
                    c.name,
                    missing.join(" and ")
                ),
                "Write `purpose = \"what it is for\"` and `owner = \"who answers for it\"` on the component, or remove both keys from every component to leave the convention.",
            )
            .location(Location::new(FILE_NAME, Some(header_line(&text, i))))
            .subject(format!("{FILE_NAME}: {}: {}", c.name, missing.join("+")))
            .positive_control(format!(
                "{total} component(s) of the declaration were read for both keys, and a component that carries both is not reported"
            )),
        );
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(toml: &str) -> Verdict {
        judge(&RepoView::from_files(&[(FILE_NAME, toml)]))
    }

    fn found(toml: &str) -> Vec<Finding> {
        match run(toml) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    const BOTH: &str = "[[architecture.component]]\nname = \"a\"\npaths = [\"a\"]\npurpose = \"p\"\nowner = \"o\"\n";
    const NONE: &str = "[[architecture.component]]\nname = \"b\"\npaths = [\"b\"]\n";

    #[test]
    fn a_declaration_that_has_not_adopted_the_keys_is_not_applicable() {
        let t = format!("[architecture]\n{NONE}");
        assert!(matches!(run(&t), Verdict::NotApplicable(_)));
    }

    #[test]
    fn every_component_carrying_both_is_clean() {
        let t = format!("[architecture]\n{BOTH}");
        assert!(found(&t).is_empty());
    }

    #[test]
    fn a_component_without_either_key_is_reported_at_its_header() {
        let t = format!("[architecture]\n{BOTH}{NONE}");
        let f = found(&t);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0]
            .subject
            .as_deref()
            .unwrap()
            .ends_with(": b: purpose+owner"));
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(7));
    }

    #[test]
    fn a_half_declared_or_blank_component_names_what_is_missing() {
        let t = "[architecture]\n[[architecture.component]]\nname = \"a\"\npaths = [\"a\"]\npurpose = \"p\"\n\n[[architecture.component]]\nname = \"b\"\npaths = [\"b\"]\npurpose = \"p\"\nowner = \"  \"\n";
        let f = found(t);
        assert_eq!(f.len(), 2, "{f:?}");
        assert!(f[0].subject.as_deref().unwrap().ends_with(": a: owner"));
        assert!(f[1].subject.as_deref().unwrap().ends_with(": b: owner"));
    }

    #[test]
    fn no_file_no_table_and_no_component_are_not_applicable() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("a.txt", "x")])),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            run("# nothing declared\n"),
            Verdict::NotApplicable(_)
        ));
        assert!(matches!(
            run("[architecture]\nstyle = \"layered\"\n"),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn an_unparsable_file_is_a_note_not_a_pass() {
        assert!(
            matches!(run("[architecture\n"), Verdict::Noted(f, n) if f.is_empty() && n.len() == 1)
        );
    }

    #[test]
    fn every_finding_validates() {
        let t = format!("[architecture]\n{BOTH}{NONE}");
        for f in found(&t) {
            f.validate().unwrap();
        }
    }
}
