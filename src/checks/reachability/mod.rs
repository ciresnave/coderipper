//! Reachability check (project scope): does every function in a Rust crate get called from
//! somewhere, including `pub` items that `rustc`'s own `dead_code` lint deliberately exempts.
//!
//! Rule record: `reachability` in `rules/mod.toml` (the rule catalog). Background in `docs/rules/mod.md`; it overlaps MOD-001 (partly) and RDB-006 (partly) of the owner's knowledge base, which this check covers only in part.
//!
//! **Packages with a library:** only the library is built (the `pub` downgrade would break every
//! other target that imports it by crate name). A lib item that a bin, an integration test, an
//! example or a bench names is therefore NOT reported, and neither is anything such an item reaches:
//! matching is by bare identifier (`rescue`), so it can over-rescue (a dead `new` is hidden when a bin
//! calls some other `new`) rather than reporting a live item as dead. Edges it follows, beyond a name
//! appearing in an item's source: `macro_rules!` bodies, `use a as b` renames, inherent-impl headers,
//! trait impls (live with their type), names inside format strings and path-shaped string literals
//! (`#[serde(default = "f")]`), and every non-lib target the manifest declares, custom paths included.
//! **In a workspace**, the members that depend on this package (directly or through other members, by
//! `path`) count like its own bins and tests: an item one of them names is live, so a library's API used
//! only by its sibling crates is not reported. A member that does not depend on this package rescues
//! nothing. A dependency redirected to a member's directory by the workspace root manifest's `[patch]` or
//! `[replace]` table counts as a `path` dependency; one redirected through a git source, a directory outside
//! the workspace or a `.cargo/config.toml` is not seen. What it cannot see: items used only by proc-macro or derive output that never spells their name.
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

mod diagnostics;
mod foreign;
mod rescue;
mod rewriter;
mod sentinel;

use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use diagnostics::{collect_dead_code_in, Targets};
use foreign::{dependent_member_identifiers, foreign_identifiers, lib_module_files};
use rescue::rescue;
use rewriter::rewrite_pub_to_pub_crate;
use sentinel::{inject_sentinel, SENTINEL_SYMBOL};

/// Reports functions nothing calls, including `pub` ones (see the module docs). Construct it with
/// [`ReachabilityCheck::new`] (or `Default`): the struct is `#[non_exhaustive]`, so a field can be added later.
///
/// ```compile_fail,E0423
/// let _ = coderipper::checks::ReachabilityCheck;
/// ```
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct ReachabilityCheck;

impl ReachabilityCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// "`x` is reachable only from `a`, which has no callers found within this crate" (at most three
/// names, then a count).
fn reachable_only_from(symbol: &str, from: &[String], cyclic: bool) -> String {
    const SHOWN: usize = 3;
    let shown: Vec<String> = from.iter().take(SHOWN).map(|n| format!("`{n}`")).collect();
    let mut names = shown.join(", ");
    if from.len() > SHOWN {
        names.push_str(&format!(" and {} more", from.len() - SHOWN));
    }
    if cyclic {
        // The items that call it are themselves reached only through it: "which has no callers" would be
        // false of each, so say what is true of the group.
        format!(
            "`{symbol}` is reachable only from {names}, and those are reachable only through it or each other: \
             nothing outside that group has callers found within this crate"
        )
    } else if from.len() == 1 {
        format!("`{symbol}` is reachable only from {names}, which has no callers found within this crate")
    } else {
        format!("`{symbol}` is reachable only from {names}, none of which has callers found within this crate")
    }
}

impl Check for ReachabilityCheck {
    fn id(&self) -> &'static str {
        "reachability"
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
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
             rewrite, so no result can be trusted. Cause: {}",
            result.failure_cause
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
        // Items another workspace member that depends on this package names are live too (a member is
        // judged alone, so without this a library's API used by its sibling crates looks dead).
        let (candidates, set_aside, by_members, dependents, only_from) = if has_lib {
            let lib_files = lib_module_files(&wt.root)?;
            let foreign = foreign_identifiers(&wt.root, &lib_files)?;
            let members = dependent_member_identifiers(&wt.root)?;
            let split = rescue(
                candidates,
                &foreign,
                &members.identifiers,
                &wt.root,
                &lib_files,
            )?;
            (
                split.kept,
                split.rescued.len(),
                split.rescued_by_members,
                members.names,
                split.only_from,
            )
        } else {
            (candidates, 0, 0, Vec::new(), Default::default())
        };
        let by_members_note = if by_members == 0 {
            String::new()
        } else {
            format!(
                " {by_members} of them because a workspace member that depends on this crate mentions their name \
                 (the members that do: {}).",
                dependents.join(", ")
            )
        };
        let from_members = if dependents.is_empty() {
            ""
        } else {
            ", or a workspace member that depends on it,"
        };
        let set_aside_note = if set_aside == 0 {
            String::new()
        } else {
            format!(
                " {set_aside} other candidate(s) in this crate were not reported because a file the \
                 library does not compile (a bin, test, example or bench){from_members} mentions their name.{by_members_note}"
            )
        };

        let positive_control = format!(
            "this run's own per-run sentinel (a guaranteed-dead function, `{SENTINEL_SYMBOL}`, \
             injected into `{sentinel_file}` before this build) WAS detected as dead_code -- \
             confirming the pipeline could actually see dead code in THIS run, not just a fixed \
             claim from a different one"
        );

        let findings: Vec<Finding> = candidates
            .into_iter()
            .map(|hit| {
                // An item only other reported items mention is dead BECAUSE they are: say so, instead of
                // claiming it has no callers (it has, and they are dead too).
                let origin = only_from.get(&(hit.file.clone(), hit.line, hit.symbol.clone()));
                let (summary, origin_note) = match origin {
                    Some(origin) => (
                        reachable_only_from(&hit.symbol, &origin.from, origin.cyclic),
                        " Every item that mentions it is itself reported (matching is by identifier): \
                         remove or revive those first and this one follows."
                            .to_string(),
                    ),
                    None => (
                        format!("`{}` has zero callers found within this crate", hit.symbol),
                        String::new(),
                    ),
                };
                Finding {
                    check_id: "reachability".into(),
                    severity: Severity::Medium,
                    confidence: Confidence::Medium,
                    project: project_name.clone(),
                    location: Some(Location {
                        file: hit.file.clone(),
                        line: Some(hit.line),
                    }),
                    subject: Some(hit.symbol.clone()),
                    summary,
                    detail: format!(
                        "Found via rustc's dead_code lint, with every top-level `pub` item downgraded to \
                         `pub(crate)` in a throwaway worktree, so the lint's normal `pub`-exemption doesn't \
                         hide it. File: {}, line {}.{origin_note}{set_aside_note}",
                        hit.file, hit.line
                    ),
                    positive_control: Some(positive_control.clone()),
                    member: None,
                }
            })
            .collect();

        // A run that sets candidates aside and reports nothing would otherwise print the same "no issues found" as a
        // crate with nothing dead. Say what happened, as a NOTE: it is bookkeeping about what was not reported, not a
        // defect, so it must not fail a run (it was an Info finding before 0.6.3, and Info fails a run by default).
        let mut notes = Vec::new();
        if findings.is_empty() && set_aside > 0 {
            notes.push(format!(
                "reachability: {set_aside} dead-code candidate(s) were set aside because a bin, test, example or bench of this \
                 package{from_members} reaches them by name; nothing else was found. With a \
                 library only the library is built, so an item another target uses looks dead to rustc; matching is by \
                 identifier, so a dead item that shares a name with something live is hidden too"
            ));
        }

        Ok((findings, notes))
    }
}

#[cfg(test)]
mod tests {
    use super::reachable_only_from;

    fn names(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("f{i}")).collect()
    }

    #[test]
    fn one_origin_reads_as_a_single_item_without_callers() {
        assert_eq!(
            reachable_only_from("x", &names(1), false),
            "`x` is reachable only from `f1`, which has no callers found within this crate"
        );
    }

    #[test]
    fn several_origins_are_listed_up_to_three_then_counted() {
        assert_eq!(
            reachable_only_from("x", &names(3), false),
            "`x` is reachable only from `f1`, `f2`, `f3`, none of which has callers found within this crate"
        );
        assert_eq!(
            reachable_only_from("x", &names(5), false),
            "`x` is reachable only from `f1`, `f2`, `f3` and 2 more, none of which has callers found within this crate"
        );
    }

    #[test]
    fn a_cycle_is_not_called_callerless() {
        let text = reachable_only_from("a", &names(1), true);
        assert!(text.contains("through it or each other"), "{text}");
        assert!(!text.contains("which has no callers"), "{text}");
    }
}
