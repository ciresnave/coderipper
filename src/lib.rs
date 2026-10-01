pub(crate) mod allowlist;
pub(crate) mod cargo_json;
pub mod check;
pub mod checks;
pub mod finding;
pub(crate) mod worktree;

use check::{Check, CheckContext, Tier};
use finding::Finding;

/// Every compiled-in check, in the order they run.
///
/// v1 deliberately uses a fixed list rather than dynamic plugin loading (design doc §7): there is
/// no known need yet for a check this project's own maintainers didn't write.
pub fn registered_checks() -> Vec<Box<dyn Check>> {
    vec![
        Box::new(checks::ReachabilityCheck),
        Box::new(checks::UnusedReturnValuesCheck),
    ]
}

/// Run every registered check at or below the requested tier, collect and validate their
/// findings, and return them. A check whose `run` returns an invalid absence-claim finding is
/// dropped with an error noted in `errors`, not silently included — see `Finding::validate`.
pub struct RunResult {
    pub findings: Vec<Finding>,
    pub errors: Vec<String>,
}

pub fn run_checks(ctx: &CheckContext, tier: Tier, only_check_id: Option<&str>) -> RunResult {
    let mut findings = Vec::new();
    let mut errors = Vec::new();

    for check in registered_checks() {
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
                for f in raw_findings {
                    match f.validate() {
                        Ok(valid) => findings.push(valid),
                        Err(e) => errors.push(format!("{}: {e}", check.id())),
                    }
                }
            }
            Err(e) => errors.push(format!("{} failed to run: {e}", check.id())),
        }
    }

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
        assert_eq!(ids, vec!["reachability", "unused-return-values"]);
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
