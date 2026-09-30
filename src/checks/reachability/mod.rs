mod allowlist;
mod diagnostics;
mod rewriter;
mod worktree;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use allowlist::Allowlist;
use diagnostics::collect_dead_code;
use worktree::RewrittenWorktree;

pub struct ReachabilityCheck;

impl Check for ReachabilityCheck {
    fn id(&self) -> &'static str {
        "reachability"
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let wt = RewrittenWorktree::create(&ctx.project_root)?;
        let result = collect_dead_code(&wt.root)?;

        if result.build_failed_for_other_reasons {
            // Per Review Focus: don't manufacture dead-code findings about a crate that didn't even
            // compile. A future iteration could surface this as its own (non-absence) finding.
            return Ok(Vec::new());
        }

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let project_name = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let findings = result
            .hits
            .into_iter()
            .filter(|hit| !allowlist.is_allowed("reachability", &hit.file, &hit.symbol))
            .map(|hit| Finding {
                check_id: "reachability".into(),
                severity: Severity::Medium,
                confidence: Confidence::Medium,
                project: project_name.clone(),
                location: Some(Location {
                    file: hit.file.clone(),
                    line: Some(hit.line),
                }),
                summary: format!("`{}` has zero callers found within this crate", hit.symbol),
                detail: format!(
                    "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
                     `pub(crate)` in a throwaway worktree, so the lint's normal `pub`-exemption doesn't \
                     hide it. File: {}, line {}.",
                    hit.file, hit.line
                ),
                positive_control: Some(
                    "the same rewrite-and-build pipeline finds no dead_code warning for a pub item \
                     that IS called elsewhere in this crate (see used_function/caller in this check's \
                     own test fixture, reachable via fn main) -- confirming the pipeline can see a \
                     real caller when one exists"
                        .to_string(),
                ),
            })
            .collect();

        Ok(findings)
    }
}
