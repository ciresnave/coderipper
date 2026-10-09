//! The language-neutral module (multi-language P3, PR 1): its five rules, over the real fixtures in `conformance/`, through the
//! conformance runner and through the host.

use coderipper::check::{CheckContext, Tier};
use coderipper::conformance::{run, Options, Verdict};
use coderipper::module::{Module, NeutralModule};
use std::path::{Path, PathBuf};

const RULES: [&str; 5] = ["SUP-001", "SUP-011", "DOC-005", "DOC-009", "WSP-001"];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

fn proofs() -> Vec<(String, Verdict)> {
    run(&NeutralModule::new(), &Options::new(fixtures()))
        .expect("the runner works on the repository's fixtures")
        .rules
        .into_iter()
        .map(|r| (r.rule, r.verdict))
        .collect()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn each_of_the_five_claims_is_earned_by_its_fixtures() {
    let proofs = proofs();
    assert_eq!(proofs.len(), 5, "one verdict per claimed rule: {proofs:?}");
    for rule in RULES {
        let verdict = proofs.iter().find(|(r, _)| r == rule).map(|(_, v)| v);
        assert_eq!(verdict, Some(&Verdict::Proven), "{rule}: {proofs:?}");
    }
}

#[test]
fn the_modules_proof_claims_are_exactly_the_rules_the_fixtures_prove() {
    let hello = NeutralModule::new().describe().unwrap();
    let mut claimed: Vec<String> = hello
        .rules
        .iter()
        .filter(|r| r.proof.is_some())
        .map(|r| r.id.clone())
        .collect();
    let mut proven: Vec<String> = proofs()
        .into_iter()
        .filter(|(_, v)| *v == Verdict::Proven)
        .map(|(r, _)| r)
        .collect();
    claimed.sort();
    proven.sort();
    assert!(!proven.is_empty(), "positive control: some rule is proven");
    assert_eq!(claimed, proven);
}

/// The controls that the five proofs above are not vacuous: a rule that reports nothing, or always reports, cannot pass both
/// halves. Here the defect is put into the clean twin, and then removed from the defective project.
#[test]
fn a_clean_twin_that_gains_the_defect_fails_and_a_defect_that_is_removed_fails() {
    for rule in RULES {
        let source = fixtures().join(rule);
        // the defect in the twin: both directories hold the defective project; the twin still expects nothing
        let polluted = tempfile::tempdir().unwrap();
        for kind in ["defective", "clean"] {
            copy_dir(
                &source.join("defective"),
                &polluted.path().join(rule).join(kind),
            );
        }
        std::fs::copy(
            source.join("clean/expect.toml"),
            polluted.path().join(rule).join("clean/expect.toml"),
        )
        .unwrap();
        let result = run(
            &NeutralModule::new(),
            &Options::new(polluted.path()).rule(rule),
        )
        .unwrap();
        match &result.rules[0].verdict {
            Verdict::Failed(reasons) => assert!(
                reasons.iter().any(|r| r.contains("clean twin")),
                "{rule}: {reasons:?}"
            ),
            other => panic!("{rule}: a clean twin with the defect must fail, got {other:?}"),
        }

        // the defect removed: both directories hold the clean project; the 'defective' one still expects the finding
        let repaired = tempfile::tempdir().unwrap();
        for kind in ["defective", "clean"] {
            copy_dir(
                &source.join("clean"),
                &repaired.path().join(rule).join(kind),
            );
        }
        std::fs::copy(
            source.join("defective/expect.toml"),
            repaired.path().join(rule).join("defective/expect.toml"),
        )
        .unwrap();
        let result = run(
            &NeutralModule::new(),
            &Options::new(repaired.path()).rule(rule),
        )
        .unwrap();
        match &result.rules[0].verdict {
            Verdict::Failed(reasons) => assert!(
                reasons.iter().any(|r| r.contains("not reported")),
                "{rule}: {reasons:?}"
            ),
            other => panic!("{rule}: a repaired defect must fail, got {other:?}"),
        }
    }
}

#[test]
fn a_directory_that_is_not_a_git_repository_is_an_error_for_every_rule_never_clean() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = CheckContext::new(dir.path());
    let result = coderipper::run_module(
        &NeutralModule::new(),
        &ctx,
        Tier::Fast,
        coderipper::module::Limits::default(),
    );
    assert_eq!(result.errors.len(), 5, "{:?}", result.errors);
    assert!(result.findings.is_empty());
    assert!(result
        .outcomes
        .iter()
        .all(|o| o.outcome == coderipper::coverage::RunOutcome::CouldNotRun));
}

#[test]
fn the_host_runs_the_module_and_reports_findings_under_the_catalog_ids() {
    let project = tempfile::tempdir().unwrap();
    copy_dir(&fixtures().join("WSP-001/defective"), project.path());
    // the fixture's answer key is not part of the project
    std::fs::remove_file(project.path().join("expect.toml")).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t.invalid",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(project.path())
            .status()
            .unwrap();
        assert!(status.success(), "{args:?}");
    }
    let ctx = CheckContext::new(project.path());
    let result = coderipper::run_module(
        &NeutralModule::new(),
        &ctx,
        Tier::Fast,
        coderipper::module::Limits::default(),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let ids: Vec<&str> = result
        .findings
        .iter()
        .map(|f| f.check_id.as_str())
        .collect();
    assert_eq!(ids, vec!["WSP-001"], "{:?}", result.findings);
    // the five rules were asked for; four did not apply or found nothing
    assert_eq!(result.outcomes.len(), 5, "{:?}", result.outcomes);
}

#[test]
fn a_project_below_the_repository_root_reads_its_own_files_even_with_a_byte_order_mark() {
    let repo = tempfile::tempdir().unwrap();
    let project = repo.path().join("proj");
    std::fs::create_dir_all(&project).unwrap();
    // a BOM before the first line must not hide the default owner; and the root's own file must not be what is read
    std::fs::write(project.join("CODEOWNERS"), "\u{feff}* @owner\n").unwrap();
    std::fs::write(repo.path().join("README.md"), "root\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.email=t@t.invalid",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "x",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(repo.path())
            .status()
            .unwrap();
        assert!(status.success(), "{args:?}");
    }
    let run_in = |dir: &Path| {
        coderipper::run_module(
            &NeutralModule::new(),
            &CheckContext::new(dir),
            Tier::Fast,
            coderipper::module::Limits::default(),
        )
    };
    let below = run_in(&project);
    assert!(below.errors.is_empty(), "{:?}", below.errors);
    assert!(below.findings.is_empty(), "{:?}", below.findings);
    // positive control: the same rule does speak about the repository root, which has no CODEOWNERS
    let root = run_in(repo.path());
    assert_eq!(root.findings.len(), 1, "{:?}", root.findings);
    assert_eq!(root.findings[0].check_id, "WSP-001");
}
