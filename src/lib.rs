pub(crate) mod allowlist;
pub mod build_cache;
pub(crate) mod cargo_json;
pub mod check;
pub mod checks;
pub mod finding;
pub mod github;
pub(crate) mod package;
pub(crate) mod session;
pub(crate) mod suppression;
pub(crate) mod worktree;

use allowlist::Allowlist;
use check::{Check, CheckContext, Tier};
use finding::Finding;
use suppression::Suppression;

/// Every compiled-in check, in the order they run.
///
/// v1 deliberately uses a fixed list rather than dynamic plugin loading (design doc §7): there is
/// no known need yet for a check this project's own maintainers didn't write.
pub fn registered_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(checks::ReachabilityCheck),
        Box::new(checks::UnusedReturnValuesCheck),
        Box::new(checks::UnusedParametersCheck),
        Box::new(checks::VersionConsistencyCheck),
        Box::new(checks::CiProtectionPresenceCheck::new()),
    ]
}

/// Run every registered check at or below the requested tier, collect and validate their
/// findings, apply the project's allowlist to them (checks return RAW findings; suppression is the
/// host's job, see `suppression`), and return what is left plus one `Info` finding per allowlist
/// entry that no longer suppresses anything. A check whose `run` returns an invalid absence-claim finding is
/// dropped with an error noted in `errors`, not silently included — see `Finding::validate`.
pub struct RunResult {
    pub findings: Vec<Finding>,
    pub errors: Vec<String>,
}

/// Which checks a run takes, by what they judge (see [`check::Unit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitFilter {
    /// Every check, whatever it judges (what `run_checks` always did).
    Any,
    /// Only the checks that judge this unit.
    Only(check::Unit),
}

pub fn run_checks(ctx: &CheckContext, tier: Tier, only_check_id: Option<&str>) -> RunResult {
    run_checks_over(
        &registered_checks(),
        ctx,
        tier,
        only_check_id,
        UnitFilter::Any,
    )
}

/// `run_checks` over an explicit list, so tests can drive the loop with fake checks.
fn run_checks_over(
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    units: UnitFilter,
) -> RunResult {
    let mut findings = Vec::new();
    let mut errors = Vec::new();

    // A malformed allowlist must not swallow the findings: report it, and run unsuppressed.
    let allowlist = Allowlist::load(&ctx.project_root).unwrap_or_else(|e| {
        errors.push(format!("allowlist: {e}"));
        Allowlist::empty()
    });
    let mut suppression = Suppression::new(&allowlist);

    let registered: Vec<&str> = checks.iter().map(|c| c.id()).collect();

    for check in checks {
        if let UnitFilter::Only(unit) = units {
            if check.unit() != unit {
                continue;
            }
        }
        if let Some(id) = only_check_id {
            if check.id() != id {
                continue;
            }
        } else if check.tier() != tier && tier == Tier::Fast {
            // Sweep mode runs everything; fast mode runs only fast-tier checks.
            continue;
        }

        match check.run(ctx) {
            Ok(raw_findings) => {
                let mut valid = Vec::new();
                let mut all_valid = true;
                for f in raw_findings {
                    match f.validate() {
                        Ok(f) => valid.push(f),
                        Err(e) => {
                            all_valid = false;
                            errors.push(format!("{}: {e}", check.id()));
                        }
                    }
                }
                // A check that produced an invalid finding is not trusted to have completed, so
                // its allowlist entries are not judged either.
                if all_valid {
                    suppression.mark_completed(check.id(), valid.len());
                }
                findings.extend(suppression.apply(valid));
            }
            Err(e) => errors.push(format!("{} failed to run: {e}", check.id())),
        }
    }

    let project = ctx
        .project_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    findings.extend(suppression.stale_findings(&registered, &project));

    RunResult { findings, errors }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_check_is_actually_registered() {
        // Review finding: this test used to run the real ReachabilityCheck against
        // std::env::current_dir() (the shared repo itself) and assert zero findings/errors --
        // which only passed because that self-scan silently failed (see the reachability check's
        // own build-failure handling) and looked like "nothing registered". Now that a check IS
        // registered, assert that directly instead of running it against a real repo from a unit
        // test -- a unit test has no business writing into a shared .git via `git worktree add`.
        let checks = registered_checks();
        let ids: Vec<_> = checks.iter().map(|c| c.id()).collect();
        assert_eq!(
            ids,
            vec![
                "reachability",
                "unused-return-values",
                "unused-parameters",
                "version-consistency",
                "ci-protection-presence"
            ]
        );
    }

    /// A check that only says what it judges.
    struct UnitFake {
        id: &'static str,
        unit: check::Unit,
    }

    impl Check for UnitFake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn scope(&self) -> check::Scope {
            check::Scope::Project
        }
        fn network(&self) -> check::Network {
            check::Network::LocalOnly
        }
        fn unit(&self) -> check::Unit {
            self.unit
        }
        fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
            Ok(vec![Finding {
                check_id: self.id.into(),
                severity: finding::Severity::Info,
                confidence: finding::Confidence::High,
                project: "p".into(),
                location: None,
                subject: Some("s".into()),
                summary: "ran".into(),
                detail: "ran".into(),
                positive_control: Some("ran".into()),
            }])
        }
    }

    fn ran(result: &RunResult) -> Vec<String> {
        let mut ids: Vec<String> = result.findings.iter().map(|f| f.check_id.clone()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_package_unit_filter_skips_repository_checks_and_the_reverse() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = CheckContext {
            project_root: tmp.path().to_path_buf(),
            portfolio_root: tmp.path().to_path_buf(),
        };
        let checks: Vec<Box<dyn Check>> = vec![
            Box::new(UnitFake {
                id: "per-package",
                unit: check::Unit::Package,
            }),
            Box::new(UnitFake {
                id: "per-repository",
                unit: check::Unit::Repository,
            }),
        ];
        let any = run_checks_over(&checks, &ctx, Tier::Fast, None, UnitFilter::Any);
        assert_eq!(
            ran(&any),
            vec!["per-package", "per-repository"],
            "Any is what run_checks always did"
        );
        let package = run_checks_over(
            &checks,
            &ctx,
            Tier::Fast,
            None,
            UnitFilter::Only(check::Unit::Package),
        );
        assert_eq!(ran(&package), vec!["per-package"]);
        let repository = run_checks_over(
            &checks,
            &ctx,
            Tier::Fast,
            None,
            UnitFilter::Only(check::Unit::Repository),
        );
        assert_eq!(ran(&repository), vec!["per-repository"]);
    }

    #[test]
    fn the_registered_checks_declare_what_they_judge() {
        let judged: Vec<(&str, check::Unit)> = registered_checks()
            .iter()
            .map(|c| (c.id(), c.unit()))
            .collect();
        assert_eq!(
            judged,
            vec![
                ("reachability", check::Unit::Package),
                ("unused-return-values", check::Unit::Package),
                ("unused-parameters", check::Unit::Package),
                // reads the whole workspace's versions: from a member it only says "run me on the root"
                ("version-consistency", check::Unit::Repository),
                // reads one GitHub repository's settings: one finding per repository, not per member
                ("ci-protection-presence", check::Unit::Repository),
            ]
        );
    }

    struct Fake {
        id: &'static str,
        result: fn() -> anyhow::Result<Vec<Finding>>,
    }

    impl Check for Fake {
        fn id(&self) -> &'static str {
            self.id
        }
        fn scope(&self) -> check::Scope {
            check::Scope::Project
        }
        fn network(&self) -> check::Network {
            check::Network::LocalOnly
        }
        fn run(&self, _ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
            (self.result)()
        }
    }

    fn fake_finding(summary: &str, control: Option<&str>) -> Finding {
        Finding {
            check_id: "fake".into(),
            severity: finding::Severity::Low,
            confidence: finding::Confidence::High,
            project: "p".into(),
            location: Some(finding::Location {
                file: "src/x.rs".into(),
                line: Some(1),
            }),
            subject: Some("present".into()),
            summary: summary.into(),
            detail: "d".into(),
            positive_control: control.map(str::to_string),
        }
    }

    /// Runs `fake` over a project whose allowlist has one entry that matches nothing.
    fn run_fake_with_an_unmatched_entry(fake: Fake) -> RunResult {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".coderipper.toml"),
            "[[allow]]\ncheck = \"fake\"\nfile = \"src/x.rs\"\nsymbol = \"gone\"\nreason = \"r\"\n",
        )
        .unwrap();
        let ctx = CheckContext {
            project_root: tmp.path().to_path_buf(),
            portfolio_root: tmp.path().to_path_buf(),
        };
        run_checks_over(&[Box::new(fake)], &ctx, Tier::Fast, None, UnitFilter::Any)
    }

    #[test]
    fn a_check_that_completes_has_its_unmatched_entry_reported() {
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || Ok(vec![fake_finding("a plain finding", None)]),
        });
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let stale: Vec<_> = r
            .findings
            .iter()
            .filter(|f| f.check_id == "allowlist")
            .collect();
        assert_eq!(stale.len(), 1, "{:?}", r.findings);
    }

    #[test]
    fn a_check_that_returned_an_invalid_finding_is_not_judged() {
        // Review finding: nothing exercised `all_valid`. An absence claim with no positive control
        // is rejected; the check did not complete cleanly, so its entries must not be called stale.
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || Ok(vec![fake_finding("zero callers found", None)]),
        });
        assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
        assert!(
            r.findings.iter().all(|f| f.check_id != "allowlist"),
            "{:?}",
            r.findings
        );
    }

    #[test]
    fn a_check_that_errors_is_not_judged() {
        let r = run_fake_with_an_unmatched_entry(Fake {
            id: "fake",
            result: || anyhow::bail!("boom"),
        });
        assert_eq!(r.errors.len(), 1);
        assert!(r.findings.is_empty(), "{:?}", r.findings);
    }

    #[test]
    fn unknown_check_id_matches_nothing_without_running_anything() {
        // A bogus id should short-circuit before any check's (potentially expensive, real-build)
        // `run` is ever called -- verified by using a path that would fail if `run` were invoked.
        let ctx = CheckContext {
            project_root: std::path::PathBuf::from("/does/not/exist"),
            portfolio_root: std::path::PathBuf::from("/does/not/exist"),
        };
        let result = run_checks(&ctx, Tier::Fast, Some("no-such-check"));
        assert!(result.findings.is_empty());
        assert!(result.errors.is_empty());
    }
}
