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

/// What a `--workspace` run did, beyond its findings and errors.
pub struct WorkspaceRun {
    pub result: RunResult,
    /// Members analysed (a poisoned session stops the loop early).
    pub members: usize,
    /// Members whose run reported at least one error.
    pub members_with_errors: usize,
}

/// Runs the checks over every member of the cargo workspace containing `ctx.project_root` (the root or any member).
///
/// Checks that judge the whole repository ([`check::Unit::Repository`]) run ONCE, at the workspace root. Checks that
/// judge a package run once per member, in sorted order, inside one session checkout (see `session`), each exactly as
/// `--project <member>` runs them: the member's own `.coderipper.toml`, the same stale-entry judgement. Every finding
/// from a member carries that member's package name. An error in one member does not stop the others; a session
/// whose restore failed does (nothing is analysed in a dirty checkout). `on_member(name, i, n)` is told as each member
/// starts, so a long run can show where it is.
pub fn run_workspace(
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    on_member: &mut dyn FnMut(&str, usize, usize),
) -> WorkspaceRun {
    run_workspace_over(&registered_checks(), ctx, tier, only_check_id, on_member)
}

fn run_workspace_over(
    checks: &[Box<dyn Check>],
    ctx: &CheckContext,
    tier: Tier,
    only_check_id: Option<&str>,
    on_member: &mut dyn FnMut(&str, usize, usize),
) -> WorkspaceRun {
    let failed = |message: String| WorkspaceRun {
        result: RunResult {
            findings: Vec::new(),
            errors: vec![message],
        },
        members: 0,
        members_with_errors: 0,
    };
    let (root, members) = match package::workspace_members(&ctx.project_root) {
        Ok(found) => found,
        Err(e) => return failed(e.to_string()),
    };

    let mut findings = Vec::new();
    let mut errors = Vec::new();
    // The same allowlist file can be judged twice (a root package is both the repository's unit and a member): a stale
    // or unknown-check entry is reported once.
    let mut judged = std::collections::HashSet::new();

    let repository = run_checks_over(
        checks,
        &CheckContext {
            project_root: root.clone(),
            portfolio_root: ctx.portfolio_root.clone(),
        },
        tier,
        only_check_id,
        UnitFilter::Only(check::Unit::Repository),
    );
    for finding in repository.findings {
        if finding.check_id != suppression::ALLOWLIST_CHECK_ID
            || judged.insert(allowlist_key(&finding))
        {
            findings.push(finding);
        }
    }
    errors.extend(repository.errors);

    let session = match session::Session::open(&root) {
        Ok(session) => session,
        Err(e) => {
            errors.push(format!("cannot open the workspace session: {e}"));
            return WorkspaceRun {
                result: RunResult { findings, errors },
                members: 0,
                members_with_errors: 0,
            };
        }
    };

    let total = members.len();
    let mut analysed = 0;
    let mut with_errors = 0;
    session::with_session(&session, || {
        for (index, member) in members.iter().enumerate() {
            on_member(&member.name, index + 1, total);
            let run = run_checks_over(
                checks,
                &CheckContext {
                    project_root: member.dir.clone(),
                    portfolio_root: ctx.portfolio_root.clone(),
                },
                tier,
                only_check_id,
                UnitFilter::Only(check::Unit::Package),
            );
            analysed += 1;
            if !run.errors.is_empty() {
                with_errors += 1;
            }
            for mut finding in run.findings {
                if finding.check_id == suppression::ALLOWLIST_CHECK_ID
                    && !judged.insert(allowlist_key(&finding))
                {
                    continue;
                }
                finding.member = Some(member.name.clone());
                findings.push(finding);
            }
            // when a restore failed for good the session's message replaces the member's own errors: it is the cause
            if let Some(why) = session.poison_message() {
                let rest: Vec<&str> = members[index + 1..]
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect();
                errors.push(format!(
                    "{}: {why}; not analysed: {}",
                    member.name,
                    if rest.is_empty() {
                        "(no more members)".to_string()
                    } else {
                        rest.join(", ")
                    }
                ));
                with_errors += 1;
                return;
            }
            errors.extend(
                run.errors
                    .into_iter()
                    .map(|e| format!("{}: {e}", member.name)),
            );
        }
    });

    WorkspaceRun {
        result: RunResult { findings, errors },
        members: analysed,
        members_with_errors: with_errors,
    }
}

/// What makes two allowlist-judgement findings the same one.
fn allowlist_key(finding: &Finding) -> (String, Option<String>, String) {
    (
        finding.project.clone(),
        finding.subject.clone(),
        finding.summary.clone(),
    )
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
                member: None,
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
            member: None,
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
