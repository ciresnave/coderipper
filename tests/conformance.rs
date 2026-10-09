//! Conformance (multi-language design section 6.2): a module's claim to cover a rule is earned by a fixture with a seeded
//! defect and a clean twin. These tests drive the runner with a scripted module (every verdict, cheaply) and with the
//! built-in Rust module over the fixtures in `conformance/` (the real claims).

use coderipper::check::Tier;
use coderipper::conformance::{run, Options, Verdict};
use coderipper::finding::{Confidence, Finding, Location, Severity};
use coderipper::module::{
    Capabilities, ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleClaim,
    RuleResult,
};
use std::path::Path;
use std::sync::Mutex;

/// What the scripted module does for one rule in one project directory: findings `(file, line)`, or a rule error.
type Script = Box<dyn Fn(&str, &Path) -> Result<Vec<(String, Option<u32>)>, String> + Send + Sync>;

struct Scripted {
    claims: Vec<RuleClaim>,
    script: Script,
    /// Emit findings under this check id instead of the requested rule's (a module reporting another rule's finding).
    emit_as: Option<String>,
    /// `(rule, project had a .git, project had expect.toml)` for every run.
    seen: Mutex<Vec<(String, bool, bool)>>,
}

impl Scripted {
    fn new(claims: &[&str], script: Script) -> Self {
        Self {
            claims: claims.iter().map(|id| RuleClaim::native(*id)).collect(),
            script,
            emit_as: None,
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl Module for Scripted {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "scripted",
            "0.0.1",
            vec!["any".into()],
            vec![],
            Capabilities::default(),
            self.claims.clone(),
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let mut events = Vec::new();
        let (mut ran, mut errored, mut findings) = (0, 0, 0);
        for rule in &request.rules {
            let root = &request.project_root;
            self.seen.lock().unwrap().push((
                rule.clone(),
                root.join(".git").exists(),
                root.join("expect.toml").exists(),
            ));
            match (self.script)(rule, root) {
                Ok(found) => {
                    ran += 1;
                    findings += found.len();
                    let count = found.len();
                    for (file, line) in found {
                        events.push(Event::Finding(Box::new(
                            Finding::new(
                                self.emit_as.as_deref().unwrap_or(rule),
                                Severity::Medium,
                                Confidence::High,
                                "p",
                                "s",
                                "d",
                            )
                            .location(Location::new(file, line)),
                        )));
                    }
                    events.push(Event::RuleResult(RuleResult::ran(rule, count)));
                }
                Err(detail) => {
                    errored += 1;
                    events.push(Event::RuleResult(RuleResult::error(
                        rule,
                        ErrorKind::Internal,
                        detail,
                    )));
                }
            }
        }
        let mut summary = ModuleSummary::default();
        summary.rules_requested = request.rules.len();
        summary.rules_ran = ran;
        summary.rules_errored = errored;
        summary.findings = findings;
        events.push(Event::Summary(summary));
        let mut output = ModuleOutput::from_events(events);
        output.in_process = true;
        output
    }
}

/// A script that reports `bad.txt:1` when the project has a `bad.txt`, and nothing otherwise.
fn reports_bad_txt() -> Script {
    Box::new(|_, root| {
        Ok(if root.join("bad.txt").exists() {
            vec![("bad.txt".into(), Some(1))]
        } else {
            vec![]
        })
    })
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

const EXPECT_BAD: &str = "[[expect]]\nrule = \"r1\"\nfile = \"bad.txt\"\nline = 1\n";

/// A fixtures directory with a correct pair for rule `r1`.
fn fixtures_with_pair() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            ("r1/defective/bad.txt", "oops\n"),
            ("r1/defective/expect.toml", EXPECT_BAD),
            ("r1/clean/ok.txt", "fine\n"),
            ("r1/clean/expect.toml", ""),
        ],
    );
    dir
}

fn verdict_of(module: &dyn Module, fixtures: &Path, rule: &str) -> Verdict {
    let result = run(module, &Options::new(fixtures)).expect("the runner itself works");
    result
        .rules
        .into_iter()
        .find(|r| r.rule == rule)
        .unwrap_or_else(|| panic!("no verdict for {rule}"))
        .verdict
}

#[test]
fn a_rule_that_reports_the_seeded_defect_and_not_its_twin_is_proven() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1"], reports_bad_txt());
    assert_eq!(verdict_of(&module, fixtures.path(), "r1"), Verdict::Proven);
}

#[test]
fn each_fixture_runs_as_a_git_repository_without_its_expectations() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1"], reports_bad_txt());
    verdict_of(&module, fixtures.path(), "r1");
    let seen = module.seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "one run per fixture: {seen:?}");
    assert!(
        seen.iter().all(|(_, git, expect)| *git && !*expect),
        "{seen:?}"
    );
}

#[test]
fn a_rule_that_misses_the_seeded_defect_fails() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1"], Box::new(|_, _| Ok(vec![])));
    match verdict_of(&module, fixtures.path(), "r1") {
        Verdict::Failed(reasons) => {
            assert!(reasons.iter().any(|r| r.contains("bad.txt")), "{reasons:?}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_rule_that_reports_the_wrong_line_fails() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            Ok(if root.join("bad.txt").exists() {
                vec![("bad.txt".into(), Some(2))]
            } else {
                vec![]
            })
        }),
    );
    assert!(matches!(
        verdict_of(&module, fixtures.path(), "r1"),
        Verdict::Failed(_)
    ));
}

#[test]
fn a_rule_that_reports_something_in_the_clean_twin_fails() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, _| Ok(vec![("bad.txt".into(), Some(1))])),
    );
    match verdict_of(&module, fixtures.path(), "r1") {
        Verdict::Failed(reasons) => {
            assert!(reasons.iter().any(|r| r.contains("clean")), "{reasons:?}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_finding_the_expectations_do_not_explain_fails() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            Ok(if root.join("bad.txt").exists() {
                vec![("bad.txt".into(), Some(1)), ("elsewhere.txt".into(), None)]
            } else {
                vec![]
            })
        }),
    );
    match verdict_of(&module, fixtures.path(), "r1") {
        Verdict::Failed(reasons) => {
            assert!(
                reasons.iter().any(|r| r.contains("elsewhere.txt")),
                "{reasons:?}"
            )
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_rule_that_cannot_run_on_its_fixture_is_an_error_not_a_failed_claim() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1"], Box::new(|_, _| Err("tool missing".into())));
    match verdict_of(&module, fixtures.path(), "r1") {
        Verdict::Errored(reasons) => assert!(
            reasons.iter().any(|r| r.contains("tool missing")),
            "{reasons:?}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_claimed_rule_with_no_fixture_is_unproven_not_proven() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1", "r2"], reports_bad_txt());
    assert_eq!(
        verdict_of(&module, fixtures.path(), "r2"),
        Verdict::NoFixture
    );
}

#[test]
fn a_defective_fixture_without_a_twin_proves_nothing() {
    // The absence-claim discipline: a query that finds something proves nothing without a clean twin to find nothing in.
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &[
            ("r1/defective/bad.txt", "x"),
            ("r1/defective/expect.toml", EXPECT_BAD),
        ],
    );
    let module = Scripted::new(&["r1"], reports_bad_txt());
    assert_eq!(verdict_of(&module, dir.path(), "r1"), Verdict::NoFixture);
}

#[test]
fn a_rule_the_module_marks_not_applicable_is_not_judged() {
    let fixtures = fixtures_with_pair();
    let mut module = Scripted::new(&["r1"], reports_bad_txt());
    let mut na = RuleClaim::native("r9");
    na.status = "not-applicable".into();
    na.reason = Some("no such construct".into());
    module.claims.push(na);
    let result = run(&module, &Options::new(fixtures.path())).unwrap();
    assert!(
        result.rules.iter().all(|r| r.rule != "r9"),
        "{:?}",
        result.rules
    );
}

#[test]
fn a_fixture_that_needs_the_network_stays_unproven_without_it() {
    let dir = fixtures_with_pair();
    write(
        dir.path(),
        &[(
            "r1/defective/expect.toml",
            &format!("needs_network = true\n{EXPECT_BAD}"),
        )],
    );
    let module = Scripted::new(&["r1"], reports_bad_txt());
    match verdict_of(&module, dir.path(), "r1") {
        Verdict::Unproven(why) => assert!(why.contains("network"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(module.seen.lock().unwrap().is_empty(), "nothing must run");
    // With the network permitted the same fixture is judged.
    let result = run(&module, &Options::new(dir.path()).network(true)).unwrap();
    assert_eq!(result.rules[0].verdict, Verdict::Proven);
}

#[test]
fn a_rule_filter_judges_only_that_rule() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1", "r2"], reports_bad_txt());
    let result = run(&module, &Options::new(fixtures.path()).rule("r1")).unwrap();
    assert_eq!(result.rules.len(), 1);
    assert_eq!(result.rules[0].rule, "r1");
}

#[test]
fn a_filter_naming_a_rule_the_module_does_not_claim_is_an_error() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(&["r1"], reports_bad_txt());
    let err = run(&module, &Options::new(fixtures.path()).rule("zzz")).expect_err("unknown rule");
    assert!(err.to_string().contains("zzz"), "{err}");
}

#[test]
fn a_malformed_expect_toml_is_an_error_naming_the_file() {
    let fixtures = fixtures_with_pair();
    write(
        fixtures.path(),
        &[(
            "r1/defective/expect.toml",
            "[[expect]]\nrule = \"r1\"\nfile = \"bad.txt\"\nbogus = 1\n",
        )],
    );
    let module = Scripted::new(&["r1"], reports_bad_txt());
    let err = run(&module, &Options::new(fixtures.path())).expect_err("bad expect.toml");
    let message = format!("{err:#}");
    assert!(
        message.contains("expect.toml") && message.contains("bogus"),
        "{message}"
    );
}

#[test]
fn a_defective_fixture_that_expects_nothing_is_an_error() {
    let fixtures = fixtures_with_pair();
    write(fixtures.path(), &[("r1/defective/expect.toml", "")]);
    let module = Scripted::new(&["r1"], reports_bad_txt());
    assert!(run(&module, &Options::new(fixtures.path())).is_err());
}

#[test]
fn a_clean_twin_that_expects_something_is_an_error() {
    let fixtures = fixtures_with_pair();
    write(fixtures.path(), &[("r1/clean/expect.toml", EXPECT_BAD)]);
    let module = Scripted::new(&["r1"], reports_bad_txt());
    assert!(run(&module, &Options::new(fixtures.path())).is_err());
}

#[test]
fn an_expectation_for_another_rule_is_an_error() {
    let fixtures = fixtures_with_pair();
    write(
        fixtures.path(),
        &[(
            "r1/defective/expect.toml",
            &EXPECT_BAD.replace("\"r1\"", "\"r2\""),
        )],
    );
    let module = Scripted::new(&["r1"], reports_bad_txt());
    assert!(run(&module, &Options::new(fixtures.path())).is_err());
}

// ---- the built-in Rust module over the repository's own fixtures -------------------------------------------------------

fn rust_proofs() -> Vec<(String, Verdict)> {
    let checks = coderipper::registered_checks();
    let module = coderipper::module::RustModule::new(&checks);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance");
    run(&module, &Options::new(fixtures))
        .expect("the runner works on the repository's fixtures")
        .rules
        .into_iter()
        .map(|r| (r.rule, r.verdict))
        .collect()
}

#[test]
fn the_rust_module_earns_four_of_its_five_claims() {
    let proofs = rust_proofs();
    let verdict = |id: &str| proofs.iter().find(|(r, _)| r == id).map(|(_, v)| v.clone());
    assert_eq!(proofs.len(), 5, "one verdict per claimed rule: {proofs:?}");
    for id in [
        "reachability",
        "unused-parameters",
        "unused-return-values",
        "version-consistency",
    ] {
        assert_eq!(verdict(id), Some(Verdict::Proven), "{id}");
    }
    // The positive control for 'proven' meaning anything: this claim has no fixture yet (its rule needs a GitHub repository,
    // a network fixture for a later step), so it is claimed but not earned.
    assert_eq!(
        verdict("ci-protection-presence"),
        Some(Verdict::NoFixture),
        "{:?}",
        verdict("ci-protection-presence")
    );
}

#[test]
fn fixtures_run_at_the_fast_tier_unless_the_network_is_allowed() {
    // The runner must not reach the network by default: the network-needing rule is the only one that could.
    assert_eq!(Options::new("x").tier(), Tier::Fast);
    assert_eq!(Options::new("x").network(true).tier(), Tier::Sweep);
}

#[test]
fn a_clean_twin_that_gains_the_defect_makes_the_real_module_fail() {
    // The control that the four proofs above are not vacuous: put the defect into the clean twin and the same run must fail.
    let fixtures = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/unused-parameters");
    for kind in ["defective", "clean"] {
        for file in ["Cargo.toml", "src/lib.rs", "expect.toml"] {
            let to = fixtures
                .path()
                .join("unused-parameters")
                .join(kind)
                .join(file);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(source.join(kind).join(file), to).unwrap();
        }
    }
    let defect = std::fs::read_to_string(source.join("defective/src/lib.rs")).unwrap();
    std::fs::write(
        fixtures.path().join("unused-parameters/clean/src/lib.rs"),
        defect,
    )
    .unwrap();
    let checks = coderipper::registered_checks();
    let module = coderipper::module::RustModule::new(&checks);
    let result = run(
        &module,
        &Options::new(fixtures.path()).rule("unused-parameters"),
    )
    .unwrap();
    match &result.rules[0].verdict {
        Verdict::Failed(reasons) => assert!(
            reasons.iter().any(|r| r.contains("clean twin")),
            "{reasons:?}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_finding_reported_under_another_rule_does_not_prove_this_one() {
    let fixtures = fixtures_with_pair();
    let mut module = Scripted::new(&["r1"], reports_bad_txt());
    module.emit_as = Some("some-other-rule".into());
    match verdict_of(&module, fixtures.path(), "r1") {
        Verdict::Failed(reasons) => assert!(
            reasons.iter().any(|r| r.contains("some-other-rule")),
            "{reasons:?}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_duplicate_finding_at_the_expected_place_is_not_explained_by_one_expectation() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            Ok(if root.join("bad.txt").exists() {
                vec![("bad.txt".into(), Some(1)), ("bad.txt".into(), Some(1))]
            } else {
                vec![]
            })
        }),
    );
    assert!(matches!(
        verdict_of(&module, fixtures.path(), "r1"),
        Verdict::Failed(_)
    ));
}

#[test]
fn an_expectation_without_a_line_still_accepts_exactly_one_finding_in_the_file() {
    let fixtures = fixtures_with_pair();
    write(
        fixtures.path(),
        &[(
            "r1/defective/expect.toml",
            "[[expect]]
rule = \"r1\"
file = \"bad.txt\"
",
        )],
    );
    let one = Scripted::new(&["r1"], reports_bad_txt());
    assert_eq!(verdict_of(&one, fixtures.path(), "r1"), Verdict::Proven);
    let two = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            Ok(if root.join("bad.txt").exists() {
                vec![("bad.txt".into(), Some(1)), ("bad.txt".into(), Some(7))]
            } else {
                vec![]
            })
        }),
    );
    assert!(matches!(
        verdict_of(&two, fixtures.path(), "r1"),
        Verdict::Failed(_)
    ));
}

#[test]
fn a_finding_with_no_location_cannot_satisfy_an_expectation() {
    let fixtures = fixtures_with_pair();
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            // A scripted finding always has a location, so model "none" with an empty file name, which is what the runner sees.
            Ok(if root.join("bad.txt").exists() {
                vec![(String::new(), None)]
            } else {
                vec![]
            })
        }),
    );
    assert!(matches!(
        verdict_of(&module, fixtures.path(), "r1"),
        Verdict::Failed(_)
    ));
}

#[test]
fn a_fixture_that_carries_its_own_git_directory_is_an_error() {
    let fixtures = fixtures_with_pair();
    write(
        fixtures.path(),
        &[(
            "r1/clean/.git/HEAD",
            "ref: refs/heads/main
",
        )],
    );
    let module = Scripted::new(&["r1"], reports_bad_txt());
    let err = run(&module, &Options::new(fixtures.path())).expect_err(".git in a fixture");
    assert!(format!("{err:#}").contains(".git"), "{err:#}");
}

#[test]
fn expectations_are_assigned_one_to_one_whatever_their_order() {
    // A file-wide expectation listed first must not consume the finding the line-specific one needs.
    let fixtures = fixtures_with_pair();
    write(
        fixtures.path(),
        &[(
            "r1/defective/expect.toml",
            "[[expect]]
rule = \"r1\"
file = \"bad.txt\"

[[expect]]
rule = \"r1\"
file = \"bad.txt\"
line = 1
",
        )],
    );
    let module = Scripted::new(
        &["r1"],
        Box::new(|_, root| {
            Ok(if root.join("bad.txt").exists() {
                vec![("bad.txt".into(), Some(1)), ("bad.txt".into(), Some(5))]
            } else {
                vec![]
            })
        }),
    );
    assert_eq!(verdict_of(&module, fixtures.path(), "r1"), Verdict::Proven);
}

#[test]
fn a_claim_with_a_status_outside_the_protocol_is_an_error() {
    let fixtures = fixtures_with_pair();
    let mut module = Scripted::new(&["r1"], reports_bad_txt());
    module.claims[0].status = "bogus".into();
    let err = run(&module, &Options::new(fixtures.path())).expect_err("bad status");
    assert!(err.to_string().contains("bogus"), "{err}");
}

#[test]
fn a_symlinked_expect_toml_or_fixture_root_is_an_error() {
    #[cfg(unix)]
    fn link(from: &Path, to: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(from, to)
    }
    #[cfg(windows)]
    fn link(from: &Path, to: &Path) -> std::io::Result<()> {
        if from.is_dir() {
            std::os::windows::fs::symlink_dir(from, to)
        } else {
            std::os::windows::fs::symlink_file(from, to)
        }
    }
    let fixtures = fixtures_with_pair();
    let real = fixtures.path().join("r1/clean/expect.toml");
    let moved = fixtures.path().join("elsewhere.toml");
    std::fs::rename(&real, &moved).unwrap();
    if link(&moved, &real).is_err() {
        return; // this machine cannot make symbolic links (an unprivileged Windows account): nothing to test here
    }
    let module = Scripted::new(&["r1"], reports_bad_txt());
    let err = run(&module, &Options::new(fixtures.path())).expect_err("symlinked expect.toml");
    assert!(format!("{err:#}").contains("symbolic link"), "{err:#}");
}
