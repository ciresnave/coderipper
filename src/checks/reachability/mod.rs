mod allowlist;
mod diagnostics;
mod rewriter;
mod sentinel;
mod worktree;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use allowlist::Allowlist;
use diagnostics::collect_dead_code;
use sentinel::{inject_sentinel, SENTINEL_SYMBOL};
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
        let sentinel_file = inject_sentinel(&wt.root)?;
        let result = collect_dead_code(&wt.root)?;

        // The sentinel is a REAL per-run positive control, not a fixed narrative string: it's a
        // guaranteed-dead function injected into THIS run's own rewritten tree. If it doesn't come
        // back as a hit, this run's result can't be trusted -- whether because the build failed
        // outright (lib+bin crates importing the lib by name, confirmed on CodeRipper's own repo
        // during review: E0603 "module is private"), or for any other reason dead_code detection
        // didn't actually fire (e.g. a crate-wide #[allow(dead_code)]). Either way, reporting empty
        // findings here would be indistinguishable from "genuinely clean", which is exactly the
        // silent-false-negative this check exists to prevent elsewhere -- so it must not commit it
        // itself. Known, accepted gap this doesn't fix (see the plan/design docs): a package with a
        // separate lib target consumed by its own bin/tests will always trip this and report an
        // error rather than partial findings.
        // Check the genuine-error signal FIRST, unconditionally -- a real compile error anywhere
        // in the build (e.g. a package with a lib target consumed by its own bin/integration-tests
        // by crate name: downgrading the lib's pub items breaks E0603 in the OTHER target, even
        // though the LIB half compiles fine on its own and can still show its own sentinel as
        // confirmed) must never look like a clean result, regardless of what the sentinel found.
        anyhow::ensure!(
            !result.build_failed_for_other_reasons,
            "this crate's build reported real compiler error(s) unrelated to dead_code after the \
             rewrite -- most commonly a package with a lib target consumed by its own bin or \
             integration tests by crate name (downgrading the lib's pub items to pub(crate) breaks \
             E0603 'module is private' in the other target). Known gap, not fixed here -- see \
             docs/superpowers/specs/2026-09-30-audit-host-design.md."
        );

        // The sentinel is a REAL per-run positive control, not a fixed narrative string: it's a
        // guaranteed-dead function injected into THIS run's own rewritten tree. If it doesn't come
        // back as a hit even though the build otherwise reported no errors, dead_code detection
        // itself didn't fire for some other reason (e.g. a crate-wide #[allow(dead_code)]) --
        // reporting empty findings here would be indistinguishable from "genuinely clean", which is
        // exactly the silent-false-negative this check exists to prevent elsewhere.
        let sentinel_confirmed = result
            .hits
            .iter()
            .any(|h| h.file == sentinel_file && h.symbol == SENTINEL_SYMBOL);
        anyhow::ensure!(
            sentinel_confirmed,
            "reachability check's own per-run positive control (a known-dead sentinel function) \
             was not detected as dead code even though the build reported no errors -- dead_code \
             detection itself is suppressed in this crate (e.g. a crate-wide #[allow(dead_code)]), \
             so this run's result can't be trusted."
        );

        let allowlist = Allowlist::load(&ctx.project_root)?;
        let project_name = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let findings = result
            .hits
            .into_iter()
            .filter(|hit| hit.symbol != SENTINEL_SYMBOL) // the sentinel itself is not a real finding
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
                positive_control: Some(format!(
                    "this run's own per-run sentinel (a guaranteed-dead function, `{SENTINEL_SYMBOL}`, \
                     injected into `{sentinel_file}` before this build) WAS detected as dead_code -- \
                     confirming the pipeline could actually see dead code in THIS run, not just a fixed \
                     claim from a different one"
                )),
            })
            .collect();

        Ok(findings)
    }
}
