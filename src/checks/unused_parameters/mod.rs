//! Unused-parameters check (project scope, Rust): which function parameters are never used?
//!
//! Rule record: `unused-parameters` in `rules/rdb.toml` (the rule catalog). Background in `docs/rules/rdb.md`; it overlaps RDB-006 (partly) of the owner's knowledge base, which this check covers only in part.
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
//!   protected. A target root cargo did not build (a `[[bin]]` whose `required-features` are off) has no crate to
//!   prove anything about: the control line names it (`target not built: required-features=[...]`) instead of failing.
//! - Only `src/` is read for parameter sites: an unused parameter in `tests/`, `examples/` or `benches/` is built (and
//!   rustc warns) but is not reported, the same boundary `unused-return-values` documents.
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
use std::path::Path;

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

    fn requires_rust_crate(&self) -> bool {
        true
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
             so no result can be trusted. Cause: {}",
            build.failure_cause()
        );

        let hits = unused_variable_hits(&build.diagnostics);
        // Every target root is its own crate, so each one needs its own proof that the lint is live. A root cargo did
        // not build (a bin whose `required-features` are off) has no crate to prove anything about: it is named, not
        // blamed on the code.
        let mut not_built: Vec<String> = Vec::new();
        let mut proven = 0usize;
        for file in &sentinel_files {
            if hits
                .iter()
                .any(|h| h.name == SENTINEL_ARG && &h.file == file)
            {
                proven += 1;
                continue;
            }
            if !root_was_built(&build.built_roots, file) {
                not_built.push(format!("`{file}` ({})", why_not_built(&wt.root, file)));
                continue;
            }
            anyhow::ensure!(
                false,
                "unused-parameters' own per-run positive control (an unused parameter injected into \
                 {file}) was not reported by rustc even though the build reported no errors -- \
                 `unused_variables` is suppressed crate-wide in that target (e.g. #![allow(unused)] \
                 or #![allow(unused_variables)]), so this run's result can't be trusted."
            );
        }

        anyhow::ensure!(
            proven > 0,
            "unused-parameters built none of this package's target roots ({}), so there is nothing to \
             check and no positive control",
            not_built.join(", ")
        );
        let not_built_note = if not_built.is_empty() {
            String::new()
        } else {
            let note = format!("not checked, target not built: {}", not_built.join(", "));
            crate::build_cache::note(note.clone());
            format!("; {note}")
        };

        let project = ctx
            .project_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let positive_control = format!(
            "this run's own sentinel (parameter `{SENTINEL_ARG}` of `{SENTINEL_FN}`, injected into \
             {}) WAS reported by rustc's `unused_variables`, so the lint was live for \
             this crate in this same build{not_built_note}",
            sentinel_files
                .iter()
                .filter(|f| !not_built.iter().any(|n| n.starts_with(&format!("`{f}`"))))
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

/// Whether cargo reported a `compiler-artifact` for the target whose root is `file` (package-relative, `src/bin/x.rs`).
/// `built_roots` holds the absolute paths cargo printed, so this matches on the path's tail.
fn root_was_built(built_roots: &[String], file: &str) -> bool {
    let tail = format!("/{file}");
    built_roots.iter().any(|p| p.ends_with(&tail))
}

/// Why cargo did not build the target whose root is `file`: the `required-features` its `[[bin]]` entry in the manifest
/// under `root` names, else a plain statement that cargo reported no unit for it.
fn why_not_built(root: &Path, file: &str) -> String {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok());
    let bins = manifest
        .as_ref()
        .and_then(|m| m.get("bin"))
        .and_then(|b| b.as_array());
    for bin in bins.into_iter().flatten() {
        let Some(bin) = bin.as_table() else { continue };
        let name = bin.get("name").and_then(|n| n.as_str());
        let path = bin
            .get("path")
            .and_then(|p| p.as_str())
            .map(str::to_string)
            .or_else(|| name.map(|n| format!("src/bin/{n}.rs")));
        if path.as_deref().map(|p| p.replace('\\', "/")) != Some(file.to_string()) {
            continue;
        }
        let features: Vec<&str> = bin
            .get("required-features")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
            .filter_map(|f| f.as_str())
            .collect();
        if !features.is_empty() {
            return format!(
                "target not built: required-features=[{}]",
                features.join(", ")
            );
        }
    }
    "target not built: cargo reported no compilation unit for it".to_string()
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
