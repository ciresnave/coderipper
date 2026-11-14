// SPDX-License-Identifier: MIT OR Apache-2.0
//! `[architecture] none = [..]`: a repository that states it has none of an input (a volatile detail, an API budget, owned data, a
//! composition root) gets the rule that needs it RUN, with 0 sites by declaration, instead of "not run". Without the statement the same
//! repository is a gap, and a statement about another input excuses nothing.

mod common;

use coderipper::check::{CheckContext, Tier};
use coderipper::{run_checks, RunResult};
use common::git_repo_with;

const MANIFEST: &str = "[package]\nname = \"mycrate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn declaration(none: &str) -> String {
    let mut s = String::from("[architecture]\n");
    if !none.is_empty() {
        s.push_str(&format!("none = [{none}]\n"));
    }
    s.push_str("\n[[architecture.component]]\nname = \"mycrate\"\npaths = [\".\"]\nrole = \"policy\"\nvolatility = \"stable\"\n");
    s
}

fn run(rule: &str, none: &str) -> RunResult {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f() {}\n"),
        (".coderipper.toml", &declaration(none)),
    ]);
    run_checks(
        &CheckContext::new(repo.path().to_path_buf()),
        Tier::Fast,
        Some(rule),
    )
}

fn says(r: &RunResult, needle: &str) -> bool {
    r.notes.iter().any(|n| n.contains(needle))
}

/// The four rules, the `none` value that excuses each, and the value that does not.
const CASES: &[(&str, &str, &str)] = &[
    ("ARC-003", "\"volatile\"", "\"api_budget\""),
    ("ARC-006", "\"api_budget\"", "\"volatile\""),
    ("ARC-007", "\"owns_data\"", "\"volatile\""),
    ("ARC-012", "\"composition\"", "\"volatile\""),
];

#[test]
fn positive_control_without_the_statement_each_rule_is_not_run() {
    for (rule, _, _) in CASES {
        let r = run(rule, "");
        assert!(r.errors.is_empty(), "{rule}: {:?}", r.errors);
        assert!(
            says(&r, &format!("{rule} not run")),
            "{rule}: {:?}",
            r.notes
        );
        assert!(!says(&r, "0 sites by declaration"), "{rule}: {:?}", r.notes);
    }
}

#[test]
fn a_stated_none_runs_the_rule_with_nothing_to_compare_and_no_gap() {
    for (rule, stated, _) in CASES {
        let r = run(rule, stated);
        assert!(r.errors.is_empty(), "{rule}: {:?}", r.errors);
        assert!(
            says(
                &r,
                &format!("{rule}: the declaration says this repository has no")
            ),
            "{rule}: {:?}",
            r.notes
        );
        assert!(says(&r, "0 sites by declaration"), "{rule}: {:?}", r.notes);
        assert!(
            !says(&r, &format!("{rule} not run")),
            "{rule}: {:?}",
            r.notes
        );
        assert!(
            r.findings.iter().all(|f| f.check_id != *rule),
            "{rule}: {:?}",
            r.findings
        );
    }
}

#[test]
fn a_statement_about_another_input_excuses_nothing() {
    for (rule, _, other) in CASES {
        let r = run(rule, other);
        assert!(
            says(&r, &format!("{rule} not run")),
            "{rule}: {:?}",
            r.notes
        );
    }
}

#[test]
fn a_contradiction_is_a_configuration_error_not_a_pass() {
    let repo = git_repo_with(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f() {}\n"),
        (
            ".coderipper.toml",
            "[architecture]\nnone = [\"api_budget\"]\n[[architecture.component]]\nname = \"mycrate\"\npaths = [\".\"]\n[architecture.api_budget]\nmycrate = 5\n",
        ),
    ]);
    let r = run_checks(
        &CheckContext::new(repo.path().to_path_buf()),
        Tier::Fast,
        Some("ARC-006"),
    );
    assert!(
        r.errors.iter().any(|e| e.contains("also declares it")),
        "{:?} / {:?}",
        r.errors,
        r.notes
    );
}
