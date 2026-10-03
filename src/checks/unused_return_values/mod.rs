//! Unused-return-values check (project scope, Rust): is a function's return value ever consumed,
//! or does every caller discard it?
//!
//! Mechanism (design doc §5, as corrected by the plan): in a throwaway worktree, tag every eligible
//! function with `#[must_use = "CR:<id>"]` + `#[deprecated(note = "CR:<id>")]`, build, and join rustc's
//! `unused_must_use` (ignored call sites) with its `deprecated` (all uses) by the `<id>`.
//!
//! No visibility rewrite is involved, so unlike the reachability check this works on a package with
//! a lib target consumed by its own bin or tests.
//!
//! **Known limitations (by design for v1):**
//! - A function defined inside a macro body (`macro_rules!`) is never tagged: syn does not see
//!   inside a macro's token tree.
//! - `let _ = f();` and `_ = f();` count as a USE: the author discarded it deliberately.
//! - Analysis is HEAD-only (uncommitted edits aren't seen), as with the reachability check.
//! - Only `src/` is rewritten; calls from `tests/`, `examples/`, `benches/` still count as uses (their
//!   `use` imports are excluded, as in `src/`).
//! - Single-package projects only (a workspace root without its own `src/` errors out).

mod classify;
mod rewriter;
mod sentinel;

use crate::cargo_json::{build_all_targets, CAP_LINTS};
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::{walk_rs_files, RewrittenWorktree};
use classify::classify;
use rewriter::{annotate, use_ranges_only};
use sentinel::{inject_sentinel, SENTINEL_FN};

pub const CHECK_ID: &str = "unused-return-values";

pub struct UnusedReturnValuesCheck;

impl Check for UnusedReturnValuesCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        let mut next_id = 0;
        let mut tags = Vec::new();
        let mut use_ranges = Vec::new();
        let wt = RewrittenWorktree::create_with(&ctx.project_root, |file, source| {
            let annotated = annotate(file, source, &mut next_id)?;
            tags.extend(annotated.tags);
            use_ranges.extend(annotated.use_ranges);
            Ok(annotated.source)
        })?;
        // Imports in files that are built but not rewritten are not call sites either.
        for dir in ["tests", "examples", "benches"] {
            let dir = wt.root.join(dir);
            if !dir.is_dir() {
                continue;
            }
            for path in walk_rs_files(&dir)? {
                let relative = path
                    .strip_prefix(&wt.root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let source = std::fs::read_to_string(&path)?;
                use_ranges.extend(use_ranges_only(&relative, &source)?);
            }
        }
        let sentinel_file = inject_sentinel(&wt.root)?;
        // Besides capping lints: an `allow` (crate-wide or on one module) must not silence the two
        // lints this check counts, or uses inside it silently go uncounted.
        let rustflags = format!("{CAP_LINTS} --force-warn deprecated --force-warn unused_must_use");
        let build = build_all_targets(&wt.root, Some(wt.source_repo()), &rustflags)?;

        // A real compiler error anywhere must never look like a clean result.
        anyhow::ensure!(
            !build.is_broken(),
            "this crate's build reported real compiler error(s) (or failed without any diagnostic) \
             after the unused-return-values rewrite, so no result can be trusted"
        );

        let classification = classify(&build.diagnostics, &tags, &use_ranges);
        anyhow::ensure!(
            classification.sentinel_ok,
            "unused-return-values' own per-run positive control (a tagged sentinel function with a \
             discarded call, injected into {sentinel_file}) was not reported by BOTH `unused_must_use` \
             and `deprecated` even though the build reported no errors -- one of those lints is \
             suppressed in this crate (e.g. a crate-wide #![allow(deprecated)]), so this run's result \
             can't be trusted."
        );

        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let positive_control = format!(
            "this run's own sentinel (`{SENTINEL_FN}`, tagged exactly like every real function and \
             called once with its value discarded, injected into `{sentinel_file}`) was reported by \
             BOTH `unused_must_use` (discarded call) and `deprecated` (use) in this same build, so \
             both signals were working for this crate"
        );

        Ok(classification
            .usages
            .iter()
            .map(|u| {
                let (severity, summary) = if u.all_ignored() {
                    (
                        Severity::Medium,
                        format!(
                            "`{}`'s return value is discarded at every one of its {} call site(s)",
                            u.tag.name, u.total
                        ),
                    )
                } else {
                    (
                        Severity::Low,
                        format!(
                            "`{}`'s return value is discarded at {} of its {} call sites",
                            u.tag.name, u.ignored, u.total
                        ),
                    )
                };
                Finding {
                    check_id: CHECK_ID.into(),
                    severity,
                    confidence: Confidence::Medium,
                    project: project.clone(),
                    location: Some(Location {
                        file: u.tag.file.clone(),
                        line: Some(u.tag.line),
                    }),
                    subject: Some(u.tag.name.clone()),
                    summary,
                    detail: format!(
                        "Counted by tagging the function with `#[must_use]` and `#[deprecated]` in a \
                         throwaway worktree and comparing rustc's discarded-value reports to its \
                         use reports. `let _ = f();` counts as a use (a deliberate discard). Defined \
                         at {}:{}.",
                        u.tag.file, u.tag.line
                    ),
                    positive_control: Some(positive_control.clone()),
                }
            })
            .collect())
    }
}
