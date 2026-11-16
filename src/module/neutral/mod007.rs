// SPDX-License-Identifier: MIT OR Apache-2.0
//! MOD-007 (mechanical half): a component that declares a purpose has a root file that states one.
//!
//! **What this judges:** a component of the `.coderipper.toml` `[architecture]` declaration that carries a non-blank `purpose` (the key
//! `DOC-003` added) is a component whose authors said what it is for. Its **root file** is the first tracked file of each of its `paths`
//! among, in order: `<p>/src/lib.rs`, `<p>/src/main.rs`, `<p>/lib.rs`, `<p>/main.rs`, `<p>/mod.rs`, `<p>.rs`. A root file with no
//! non-blank `//!` (or `/*! */`) inner doc comment says nothing about the module's job, so the declared purpose has nothing to be compared
//! with: **Info** severity, **Low** confidence, one finding per such root file, at line 1, naming the component and its purpose.
//!
//! **What is not built, and why:** the other half of MOD-007 is the *judgement*: whether the stated responsibility in the module doc
//! matches the declared purpose, or joins unrelated actors. That compares two natural-language texts and, with the module's code, asks
//! whom the module serves and what drives its changes. No mechanical test of that is honest; it needs a language model or a human, so it
//! stays a named row (`MOD-007`, with `ARC-010`) and this check does not claim it. This half only finds the case where there is no
//! stated responsibility at all.
//!
//! **Adoption is by presence:** no component with a `purpose` (or no `.coderipper.toml`, no `[architecture]`) is "not applicable". A
//! component path with no root file among the candidates (a non-Rust component, a path that is not a crate or module) is not judged.
//! An unparsable `.coderipper.toml` is a note. Opt-in: it runs when named (`coderipper check MOD-007`).

use super::{RepoView, Verdict};
use crate::config::{ConfigFile, FILE_NAME};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::path::Path;

const ID: &str = crate::rule_id::MOD_007;

/// The root file of a component path, if the repository tracks one.
fn root_file(view: &RepoView, path: &str) -> Option<String> {
    let p = path.trim_matches('/').replace('\\', "/");
    let p = p.trim_start_matches("./");
    let join = |tail: &str| {
        if p.is_empty() || p == "." {
            tail.to_string()
        } else {
            format!("{p}/{tail}")
        }
    };
    let mut candidates = vec![
        join("src/lib.rs"),
        join("src/main.rs"),
        join("lib.rs"),
        join("main.rs"),
        join("mod.rs"),
    ];
    if !p.is_empty() && p != "." {
        candidates.push(format!("{p}.rs"));
    }
    candidates.into_iter().find(|c| view.read(c).is_some())
}

/// Whether Rust source text holds a non-blank inner doc comment.
fn has_module_doc(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start();
        t.strip_prefix("//!")
            .or_else(|| t.strip_prefix("/*!"))
            .is_some_and(|rest| !rest.trim().trim_end_matches("*/").trim().is_empty())
    })
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let Some(text) = view.read(FILE_NAME) else {
        return Verdict::NotApplicable(format!(
            "the repository tracks no {FILE_NAME}, so it declares no component purpose"
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
        return Verdict::NotApplicable("the repository declares no [architecture] table".into());
    };
    let declared: Vec<_> = arch
        .component
        .iter()
        .filter(|c| c.purpose.as_deref().is_some_and(|p| !p.trim().is_empty()))
        .collect();
    if declared.is_empty() {
        return Verdict::NotApplicable(
            "no component declares a purpose: the check is opt-in, adopted by writing `purpose` (DOC-003)"
                .into(),
        );
    }
    let mut findings = Vec::new();
    let mut roots = 0usize;
    for c in &declared {
        let purpose = c.purpose.as_deref().unwrap_or_default().trim();
        for path in &c.paths {
            let Some(root) = root_file(view, path) else {
                continue;
            };
            let Some(source) = view.read(&root) else {
                continue;
            };
            roots += 1;
            if has_module_doc(&source) {
                continue;
            }
            findings.push(
                Finding::new(
                    ID,
                    Severity::Info,
                    Confidence::Low,
                    view.name(),
                    format!(
                        "the component `{}` declares the purpose \"{purpose}\", but its root file {root} has no `//!` module documentation to compare it with",
                        c.name
                    ),
                    "State in a `//!` comment at the top of the root file what the module is for and whom it serves, so the declared purpose and the code's own account can be compared. This check does not judge whether they agree.",
                )
                .location(Location::new(root.as_str(), Some(1)))
                .subject(format!("{}: {root}", c.name))
                .positive_control(format!(
                    "{} component(s) with a purpose were read; {roots} root file(s) were found and checked for a `//!` doc, and a root with one is not reported",
                    declared.len()
                )),
            );
        }
    }
    Verdict::Findings(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECL: &str = "[architecture]\n[[architecture.component]]\nname = \"core\"\npaths = [\"core\"]\npurpose = \"parses orders\"\n";

    fn run(files: &[(&str, &str)]) -> Verdict {
        judge(&RepoView::from_files(files))
    }

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match run(files) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_root_with_a_module_doc_is_clean() {
        let f = found(&[
            (FILE_NAME, DECL),
            ("core/src/lib.rs", "//! Parses orders.\npub fn a() {}\n"),
        ]);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn a_root_with_no_or_an_empty_doc_is_reported_at_line_one() {
        for src in [
            "pub fn a() {}\n",
            "//!\n//!   \npub fn a() {}\n",
            "/*! */\n",
        ] {
            let f = found(&[(FILE_NAME, DECL), ("core/src/lib.rs", src)]);
            assert_eq!(f.len(), 1, "{src:?}: {f:?}");
            let l = f[0].location.as_ref().unwrap();
            assert_eq!((l.line, l.file.as_str()), (Some(1), "core/src/lib.rs"));
            assert!(f[0].summary.contains("parses orders"));
        }
    }

    #[test]
    fn a_module_path_resolves_to_its_file_or_mod_rs() {
        let t = DECL.replace("[\"core\"]", "[\"src/core\"]");
        assert_eq!(
            found(&[(FILE_NAME, &t), ("src/core.rs", "fn a() {}\n")]).len(),
            1
        );
        assert_eq!(
            found(&[(FILE_NAME, &t), ("src/core/mod.rs", "fn a() {}\n")]).len(),
            1
        );
        assert!(found(&[(FILE_NAME, &t), ("src/core/mod.rs", "//! x\n")]).is_empty());
    }

    #[test]
    fn a_component_without_a_purpose_or_a_root_file_is_not_judged() {
        let none =
            "[architecture]\n[[architecture.component]]\nname = \"core\"\npaths = [\"core\"]\n";
        assert!(matches!(
            run(&[(FILE_NAME, none), ("core/src/lib.rs", "fn a() {}\n")]),
            Verdict::NotApplicable(_)
        ));
        assert!(found(&[(FILE_NAME, DECL), ("core/README.md", "x")]).is_empty());
    }

    #[test]
    fn no_file_no_table_and_an_unparsable_file_are_not_a_pass() {
        assert!(matches!(run(&[("a.txt", "x")]), Verdict::NotApplicable(_)));
        assert!(matches!(
            run(&[(FILE_NAME, "# nothing\n")]),
            Verdict::NotApplicable(_)
        ));
        assert!(
            matches!(run(&[(FILE_NAME, "[architecture\n")]), Verdict::Noted(f, n) if f.is_empty() && n.len() == 1)
        );
    }

    #[test]
    fn every_finding_validates() {
        for f in found(&[(FILE_NAME, DECL), ("core/src/lib.rs", "fn a() {}\n")]) {
            f.validate().unwrap();
        }
    }
}
