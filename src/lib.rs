pub mod check;
pub mod checks;
pub mod finding;

use check::{Check, CheckContext, Tier};
use finding::Finding;

/// Every compiled-in check, in the order they run.
///
/// v1 deliberately uses a fixed list rather than dynamic plugin loading (design doc §7): there is
/// no known need yet for a check this project's own maintainers didn't write.
pub fn registered_checks() -> Vec<Box<dyn Check>> {
    // No checks implemented yet — `reachability` (design doc §3) is next.
    Vec::new()
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
    fn running_with_no_registered_checks_returns_empty_cleanly() {
        let ctx = CheckContext {
            project_root: std::env::current_dir().unwrap(),
            portfolio_root: std::env::current_dir().unwrap(),
        };
        let result = run_checks(&ctx, Tier::Fast, None);
        assert!(result.findings.is_empty());
        assert!(result.errors.is_empty());
    }
}
