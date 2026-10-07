//! Reachability check (project scope): does every function in a Rust crate get called from
//! somewhere, including `pub` items that `rustc`'s own `dead_code` lint deliberately exempts.
//!
//! **Packages with a library:** only the library is built (the `pub` downgrade would break every
//! other target that imports it by crate name). A lib item that a bin, an integration test, an
//! example or a bench names is therefore NOT reported, and neither is anything such an item reaches:
//! matching is by bare identifier (`rescue`), so it can over-rescue (a dead `new` is hidden when a bin
//! calls some other `new`) rather than reporting a live item as dead. Edges it follows, beyond a name
//! appearing in an item's source: `macro_rules!` bodies, `use a as b` renames, inherent-impl headers,
//! trait impls (live with their type), names inside format strings and path-shaped string literals
//! (`#[serde(default = "f")]`), and every non-lib target the manifest declares, custom paths included.
//! What it cannot see: items used only by proc-macro or derive output that never spells their name.
//!
//! **Known limitations, found and confirmed during implementation and review:**
//! - **`pub` items inside bin targets are not analyzed** when the package also has a library: rustc
//!   exempts them unless downgraded, and downgrading a bin against an unmodified lib is a separate
//!   pass. A package with no library is analyzed as before (all targets).
//! - **Custom `[lib] path`** is not followed: only `src/lib.rs` counts as the library (custom paths of
//!   bins, tests, examples and benches are).
//! - **A function called only from a `#[test]`** is not rescued from `dead_code` by rustc (confirmed
//!   empirically, many independent fresh builds) -- "a test counts as reachability" (the design
//!   doc's stated intent) is not delivered for this case.
//! - **Analysis is HEAD-only**: uncommitted edits in the real working tree aren't seen (a property of
//!   using a git worktree, not a bug).
//! - **A `--project` pointing at a crate nested inside a larger repo** analyzes the enclosing repo's
//!   root, not the nested crate's.
//!
//! Full detail: the reachability plan's status note and the audit-host design, in the source repository's `docs/`.

mod diagnostics;
mod foreign;
mod rescue;
mod rewriter;
mod sentinel;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use diagnostics::{collect_dead_code_in, Targets};
use foreign::{foreign_identifiers, lib_module_files};
use rescue::rescue;
use rewriter::rewrite_pub_to_pub_crate;
use sentinel::{inject_sentinel, SENTINEL_SYMBOL};

/// Reports functions nothing calls, including `pub` ones (see the module docs).
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
        let wt = RewrittenWorktree::create_with(&ctx.project_root, |_, source| {
            Ok(rewrite_pub_to_pub_crate(source))
        })?;
        let sentinel_file = inject_sentinel(&wt.root)?;
        // With a library, build ONLY the library: downgrading its `pub` items breaks every other target
        // that imports it by crate name (E0603), so bins, tests, examples and benches are not compiled.
        // What they use is accounted for by `rescue` below instead.
        let has_lib = wt.root.join("src/lib.rs").is_file();
        let targets = if has_lib {
            Targets::LibOnly
        } else {
            Targets::All
        };
        let result = collect_dead_code_in(&wt.root, Some(wt.source_repo()), targets)?;

        // A real compile error anywhere in the build must never look like a clean result, whatever
        // the sentinel found.
        anyhow::ensure!(
            !result.build_failed_for_other_reasons,
            "this crate's build reported real compiler error(s) unrelated to dead_code after the \
             rewrite, so no result can be trusted"
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

        let project_name = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let candidates: Vec<_> = result
            .hits
            .into_iter()
            .filter(|hit| hit.symbol != SENTINEL_SYMBOL) // the sentinel itself is not a real finding
            .collect();
        // Items a target the lib build never compiled (bin, test, example, bench) names are live as
        // far as this check can tell; set them aside, and everything they reach.
        let (candidates, set_aside) = if has_lib {
            let lib_files = lib_module_files(&wt.root)?;
            let foreign = foreign_identifiers(&wt.root, &lib_files)?;
            let split = rescue(candidates, &foreign, &wt.root, &lib_files)?;
            (split.kept, split.rescued.len())
        } else {
            (candidates, 0)
        };
        let set_aside_note = if set_aside == 0 {
            String::new()
        } else {
            format!(
                " {set_aside} other candidate(s) in this crate were not reported because a file the \
                 library does not compile (a bin, test, example or bench) mentions their name."
            )
        };

        let positive_control = format!(
            "this run's own per-run sentinel (a guaranteed-dead function, `{SENTINEL_SYMBOL}`, \
             injected into `{sentinel_file}` before this build) WAS detected as dead_code -- \
             confirming the pipeline could actually see dead code in THIS run, not just a fixed \
             claim from a different one"
        );

        let mut findings: Vec<Finding> = candidates
            .into_iter()
            .map(|hit| Finding {
                check_id: "reachability".into(),
                severity: Severity::Medium,
                confidence: Confidence::Medium,
                project: project_name.clone(),
                location: Some(Location {
                    file: hit.file.clone(),
                    line: Some(hit.line),
                }),
                subject: Some(hit.symbol.clone()),
                summary: format!("`{}` has zero callers found within this crate", hit.symbol),
                detail: format!(
                    "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
                     `pub(crate)` in a throwaway worktree, so the lint's normal `pub`-exemption doesn't \
                     hide it. File: {}, line {}.{set_aside_note}",
                    hit.file, hit.line
                ),
                positive_control: Some(positive_control.clone()),
                member: None,
            })
            .collect();

        // A run that sets candidates aside and reports nothing would otherwise print the same
        // "no issues found" as a crate with nothing dead. Say what happened.
        if findings.is_empty() && set_aside > 0 {
            findings.push(Finding {
                check_id: "reachability".into(),
                severity: Severity::Info,
                confidence: Confidence::High,
                project: project_name.clone(),
                location: None,
                subject: None,
                summary: format!(
                    "{set_aside} dead-code candidate(s) were set aside because a bin, test, example or \
                     bench reaches them by name; nothing else was found"
                ),
                detail: format!(
                    "With a library, only the library is built, so an item another target uses looks \
                     dead to rustc.{set_aside_note} Matching is by identifier, so a dead item that shares \
                     a name with something live is hidden too."
                ),
                positive_control: Some(positive_control),
                member: None,
            });
        }

        Ok(findings)
    }
}
