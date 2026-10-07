//! Unused-parameters check (project scope, Rust): which function parameters are never used?
//!
//! Rust already has this lint (`unused_variables`), default-on. What it does not do is report in
//! CodeRipper's unified `Finding` shape, ignore the forced signatures of trait impls, or tell a
//! parameter from a local (rustc words both identically). So this check does not re-detect
//! anything: it builds the crate in a throwaway worktree, takes rustc's own `unused_variables`
//! diagnostics, and keeps those that land exactly on a parameter of a free function or inherent
//! method, as found by `syn` (see `sites`).
//!
//! **Known limitations / deliberate scope:**
//! - `_`-prefixed parameters are exempt by Rust convention and never reported. The spec notes the
//!   blind spot ("accepted and silently ignored") and explicitly does NOT scope it in.
//! - Trait declarations, trait default bodies, trait-impl methods and closure parameters are out of
//!   scope (see `sites`); so is anything defined inside a macro body.
//! - An explicit `#[allow(unused_variables)]` on an item is respected: it is a deliberate, visible,
//!   local decision (use the allowlist to record a reason). A CRATE-wide allow, which would silence
//!   every parameter, makes the sentinel fail and the run errors instead of reporting clean. There
//!   is one sentinel per target root (`src/lib.rs`, `src/main.rs`, `src/bin/*`), since each target is
//!   its own crate. Targets with a custom `path = ...` in `Cargo.toml` are not found and so are not
//!   protected.
//! - A finding's `subject` is qualified by INLINE `mod {}` blocks, the impl's type (generic arguments
//!   are not part of it: `impl W<u8>` and `impl W<u16>` share `W`), and enclosing functions. The
//!   file is part of an allowlist entry's identity, so the file tree does not need to be.
//! - Analysis is HEAD-only, single-package projects only, as with the other checks.

mod sentinel;
mod sites;

use crate::cargo_json::{build_all_targets, Diagnostic, CAP_LINTS};
use crate::check::{Check, CheckContext, Network, Scope};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::worktree::RewrittenWorktree;
use sentinel::{inject_sentinels, SENTINEL_ARG, SENTINEL_FN};
use sites::{find_param_sites, ParamSite};
use std::collections::BTreeSet;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = "unused-parameters";

/// Reports function parameters nothing reads (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct UnusedParametersCheck;

impl UnusedParametersCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for UnusedParametersCheck {
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
        let mut sites: Vec<ParamSite> = Vec::new();
        // The rewrite is the identity: this check reads the source, it does not change it.
        let wt = RewrittenWorktree::create_with(&ctx.project_root, |file, source| {
            sites.extend(find_param_sites(file, source)?);
            Ok(source.to_string())
        })?;
        let sentinel_files = inject_sentinels(&wt.root)?;
        // `--cap-lints=warn`: a `#![deny(warnings)]` must not fail the build and hide the lint.
        let build = build_all_targets(&wt.root, Some(wt.source_repo()), CAP_LINTS)?;

        anyhow::ensure!(
            !build.is_broken(),
            "this crate's build reported real compiler error(s) (or failed without any diagnostic), \
             so no result can be trusted"
        );

        let hits = unused_variable_hits(&build.diagnostics);
        // Every target root is its own crate, so each one needs its own proof that the lint is live.
        for file in &sentinel_files {
            anyhow::ensure!(
                hits.iter().any(|h| h.name == SENTINEL_ARG && &h.file == file),
                "unused-parameters' own per-run positive control (an unused parameter injected into \
                 {file}) was not reported by rustc even though the build reported no errors -- \
                 `unused_variables` is suppressed crate-wide in that target (e.g. #![allow(unused)] \
                 or #![allow(unused_variables)]), so this run's result can't be trusted."
            );
        }

        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let positive_control = format!(
            "this run's own sentinel (parameter `{SENTINEL_ARG}` of `{SENTINEL_FN}`, injected into \
             {}) WAS reported by rustc's `unused_variables`, so the lint was live for \
             this crate in this same build",
            sentinel_files
                .iter()
                .map(|f| format!("`{f}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let mut findings: Vec<Finding> = sites
            .iter()
            .filter(|site| {
                hits.iter()
                    .any(|h| h.file == site.file && h.line == site.line && h.column == site.column)
            })
            .map(|site| Finding {
                check_id: CHECK_ID.into(),
                severity: Severity::Low,
                confidence: Confidence::High,
                project: project.clone(),
                location: Some(Location {
                    file: site.file.clone(),
                    line: Some(site.line),
                }),
                subject: Some(site.subject()),
                summary: format!(
                    "parameter `{}` of `{}` is never used",
                    site.name, site.function
                ),
                detail: format!(
                    "Reported by rustc's own `unused_variables` lint and kept because it lands on a \
                     parameter of a free function or inherent method (not a local, closure or trait \
                     signature). Defined at {}:{}. Prefixing the name with `_` silences rustc, which \
                     this check does not look for.",
                    site.file, site.line
                ),
                positive_control: Some(positive_control.clone()),
                member: None,
            })
            .collect();
        findings.sort_by(|a, b| {
            let key = |f: &Finding| {
                f.location
                    .as_ref()
                    .map(|l| (l.file.clone(), l.line))
                    .unwrap_or_default()
            };
            key(a).cmp(&key(b))
        });
        Ok(findings)
    }
}

/// One `unused_variables` report, de-duplicated (`--all-targets` compiles shared code twice).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Hit {
    file: String,
    line: u32,
    column: u32,
    name: String,
}

fn unused_variable_hits(diagnostics: &[Diagnostic]) -> BTreeSet<Hit> {
    diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused_variables"))
        .filter_map(|d| {
            let span = d.primary_span()?;
            // "unused variable: `b`" -> b
            let name = d.message.split('`').nth(1)?.to_string();
            Some(Hit {
                file: span.file.clone(),
                line: span.line,
                column: span.column,
                name,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo_json::Span;

    fn diag(code: &str, message: &str, line: u32, column: u32) -> Diagnostic {
        Diagnostic {
            code: Some(code.into()),
            level: "warning".into(),
            message: message.into(),
            notes: vec![],
            spans: vec![Span {
                file: "src/lib.rs".into(),
                line,
                column,
                is_primary: true,
            }],
        }
    }

    #[test]
    fn only_unused_variables_diagnostics_become_hits_and_duplicates_collapse() {
        let hits = unused_variable_hits(&[
            diag("unused_variables", "unused variable: `b`", 3, 21),
            diag("unused_variables", "unused variable: `b`", 3, 21), // second target
            diag("dead_code", "function `f` is never used", 1, 1),
            diag("unused_variables", "unused variable: `c`", 4, 5),
        ]);
        let got: Vec<_> = hits
            .iter()
            .map(|h| (h.name.as_str(), h.line, h.column))
            .collect();
        assert_eq!(got, vec![("b", 3, 21), ("c", 4, 5)]);
    }
}
