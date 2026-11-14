// SPDX-License-Identifier: MIT OR Apache-2.0
//! Domain errors free of transport details (repository scope, Rust): does a policy component's public *error type* name an HTTP type, a
//! database or network driver type, or another infrastructure type?
//!
//! Rule record: `ERR-006` in `rules/err.toml`. It reads the same declaration as `ARC-004` (`[architecture]`, see
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`) and the same public-signature reader
//! (`infra_signatures.rs`), restricted to error types.
//!
//! **What it judges.** For each *policy* component (`role = "policy"`, or no role and `volatility = "stable"`), in the library
//! (`src/lib.rs`) of each of its crates at `HEAD` (test items skipped), every public item whose type name ends in `Error` (a struct's
//! public fields, an enum's variant fields, the methods of such a type are not read here: the fields and variants are what carry a
//! transport detail): the types its fields and variants name, resolved through the module's own `use` declarations. A path is a transport
//! detail when it is infrastructure for `ARC-004` (a namespace in `[architecture.volatile] namespaces` or a name in
//! `[architecture.volatile] names`, a workspace crate of a component declared volatile, an adapter or infrastructure), **or** when it
//! starts with a namespace this rule always counts as transport or driver: `http`, `hyper`, `axum`, `actix_web`, `warp`, `rocket`,
//! `tonic`, `reqwest`, `tower_http`, `sqlx`, `diesel`, `sea_orm`, `tokio_postgres`, `postgres`, `rusqlite`, `mysql`, `mysql_async`,
//! `redis`, `mongodb`, `rdkafka`, `lapin`. So an error such as `Db(sqlx::Error)` or `Http(http::StatusCode)` in a policy component is
//! reported even when the declaration names no volatile detail.
//!
//! One finding per error type site (`Type::Variant` or `Type.field`), naming every transport type it contains, at Low severity and Low
//! confidence (the catalogue record's own).
//!
//! **Relation to ARC-004.** `ARC-004` reports *any* public signature of a policy component that names infrastructure; this rule reports
//! the subset that sits in an *error* type, with the always-transport namespaces added, because a domain error that carries a driver
//! error or a status code couples every caller to one delivery mechanism. The same site can be reported by both; they are different
//! rules with different remedies (ARC-004: keep the technology out of the API; ERR-006: translate it into a domain error at the edge).
//!
//! **Weak evidence by construction.** Syntactic: a glob import does not resolve the names it brings in, a type alias in another crate or
//! a re-export through a facade hides the origin, macro-generated items (a `thiserror` `#[from]` is read, the generated impl is not) are
//! not seen, and an error type is recognised by its name ending in `Error`.
//!
//! **No declaration, or no policy component, is not clean:** the rule reports "not run" with what to add
//! ([`Check::not_configured`]).
//!
//! **Known limitations / deliberate scope:** crate granularity (module-path components are noted, not judged); only `src/` is read; an
//! error type whose name does not end in `Error` is not read.

use super::infra_signatures::{is_policy, signature_sites, Banned, Site};
use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::read_sources;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ERR_006;

/// The namespaces this rule always counts as transport or driver details, whatever the declaration says.
const TRANSPORT_NAMESPACES: &[&str] = &[
    "http",
    "hyper",
    "axum",
    "actix_web",
    "warp",
    "rocket",
    "tonic",
    "reqwest",
    "tower_http",
    "sqlx",
    "diesel",
    "sea_orm",
    "tokio_postgres",
    "postgres",
    "rusqlite",
    "mysql",
    "mysql_async",
    "redis",
    "mongodb",
    "rdkafka",
    "lapin",
];

/// Reports transport details named in the public error types of policy components (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct TransportErrorCheck;

impl TransportErrorCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ERR-006 has no policy component whose error types to read; a missing declaration is a gap, not a pass (docs/rules/err.md)".into());
    };
    if !a.component.iter().any(is_policy) {
        return Some("no [[architecture.component]] has role = \"policy\" (or no role and volatility = \"stable\"), so there is no domain component whose error types to read (docs/rules/err.md)".into());
    }
    None
}

/// The type an item label belongs to: `Type::Variant` and `Type.field` both give `Type`.
fn owner_type(label: &str) -> &str {
    label.split(['.', ':', ' ']).next().unwrap_or(label)
}

/// Whether a label is a field or variant of an error type (a name ending in `Error`).
fn in_error_type(label: &str) -> bool {
    (label.contains('.') || label.contains("::")) && owner_type(label).ends_with("Error")
}

fn judge(
    component: &str,
    manifest: &str,
    sites: &[Site],
    banned: &Banned,
    project: &str,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for s in sites.iter().filter(|s| in_error_type(&s.label)) {
        let mut hits: Vec<String> = s.paths.iter().filter_map(|p| banned.why(p)).collect();
        hits.sort();
        hits.dedup();
        if hits.is_empty() {
            continue;
        }
        let shown = hits.join(", ");
        out.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Low,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: s.file.clone(),
                line: u32::try_from(s.line).ok(),
            }),
            subject: Some(format!("{component}:{}", s.label)),
            summary: format!(
                "the error type `{}` in the policy component `{component}` names transport details: {shown}",
                owner_type(&s.label)
            ),
            detail: format!(
                "`{}` is a public error type of `{component}` (manifest {manifest}), and it names {shown}. A caller that handles it then \
                 depends on how the failure was delivered, not on what failed. Keep domain errors in domain terms and translate the driver or \
                 HTTP error into one at the edge (an adapter), so the domain does not change when the transport does. Read from the syntax \
                 only: a type alias or a re-export can hide the origin, and an error type is recognised by its name ending in `Error`.",
                s.label
            ),
            member: None,
            positive_control: None,
        });
    }
    out
}

impl Check for TransportErrorCheck {
    fn id(&self) -> &'static str {
        CHECK_ID
    }

    fn scope(&self) -> Scope {
        Scope::Project
    }

    fn unit(&self) -> Unit {
        Unit::Repository
    }

    fn requires_rust_crate(&self) -> bool {
        true
    }

    fn network(&self) -> Network {
        Network::LocalOnly
    }

    fn not_configured(&self, ctx: &CheckContext) -> Option<String> {
        match architecture::load(&ctx.project_root) {
            Ok(Some((a, _))) => missing_declaration(Some(&a)),
            Ok(None) => missing_declaration(None),
            // an unreadable declaration is an error of the run, raised by `run`
            Err(_) => None,
        }
    }

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let Some((arch, file)) = architecture::load(&ctx.project_root)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        let patterns = crate::config::arc004_patterns(&ctx.project_root)?;
        let graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = Resolved::new(&arch, &graph.members);
        let mut banned = Banned::new(&arch, &patterns, &graph.members, &resolved);
        for ns in TRANSPORT_NAMESPACES {
            banned.add_namespace(&[ns]);
        }
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        let mut error_sites = 0usize;
        for c in arch.component.iter().filter(|c| is_policy(c)) {
            if c.paths
                .iter()
                .any(|p| is_module_path(&norm(p), &graph.members))
            {
                notes.push(format!(
                    "component `{}` has module paths inside a crate; module groups are not judged in this version, only whole crates",
                    c.name
                ));
                continue;
            }
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                let files = read_sources(&ctx.project_root.join(&m.dir))?;
                let (sites, n) = signature_sites(&files, &m.dir)?;
                error_sites += sites.iter().filter(|s| in_error_type(&s.label)).count();
                notes.extend(n.into_iter().map(|s| format!("crate `{}`: {s}", m.name)));
                findings.extend(judge(&c.name, &m.manifest, &sites, &banned, &project));
            }
        }
        findings.sort_by(|a, b| (&a.subject, &a.summary).cmp(&(&b.subject, &b.summary)));
        notes.push(format!(
            "ERR-006 read {error_sites} public error-type field or variant site(s) in policy components; {} name transport details",
            findings.len()
        ));
        Ok((findings, notes))
    }
}

#[cfg(test)]
mod tests;
