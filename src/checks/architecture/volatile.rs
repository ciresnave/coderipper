// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stable policy avoids volatile detail (repository scope, Rust): does a component the project tagged `stable` name the types,
//! namespaces and build features it declared `volatile`?
//!
//! Rule record: `ARC-003` in `rules/arc.toml`. Declaration format:
//! `docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`; background: `docs/rules/arc.md`.
//!
//! **What it counts.** For each component whose `volatility` is `stable` and whose `role` is `policy` or unset, in every tracked `.rs`
//! file under the `src/` of each of its crates at `HEAD` (test items skipped): uses of a name in `[architecture.volatile] names` (any
//! identifier token equal to it), uses of a path prefix in `namespaces` (`sqlx::` matches the tokens `sqlx` `::`), and, when
//! `feature_cfg_in_policy` is set, `#[cfg(..)]`/`#[cfg_attr(..)]` attributes that test a Cargo feature (`feature = "..."`). One finding
//! per component, with the counts, the most affected files and the features; the components are ranked in a note.
//!
//! **Weak evidence by construction (medium confidence).** The count is syntactic: an identifier equal to a volatile name is counted
//! whether or not it is the declared type (a local variable called `BackendId` cannot exist in idiomatic Rust, but a field or method of
//! that name is counted), a type alias or re-export hides a use, a macro's output is not seen, and a feature-gated line is an inventory
//! of build-time variation, not proof that the feature is a backend. A `use` of a namespace and each later use of the imported name
//! are two different places. Low severity when a name or namespace is used, Info when only feature attributes are found.
//!
//! **No declaration, no volatile table, or no component tagged stable is not clean:** the rule reports "not run" with what to add
//! ([`Check::not_configured`]).
//!
//! **Known limitations / deliberate scope:**
//! - Crate granularity (components are crate directories, as in ARC-001); a component of module paths inside a crate is noted, not judged.
//! - Only `src/` is read; `build.rs`, `tests/`, `benches/` and `examples/` are not.
//! - `cfg!(feature = "..")` macro calls are not counted, only attributes.

use super::{is_module_path, norm, read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig, Role, VolatileDecl, Volatility};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::BTreeMap;
use syn::visit::Visit;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::ARC_003;

/// Reports volatile names, namespaces and feature gates inside components declared stable (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct VolatileDetailCheck;

impl VolatileDetailCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

/// What a project must declare for the rule to judge anything, or `None` when it has declared it.
fn missing_declaration(a: Option<&ArchitectureConfig>) -> Option<String> {
    let Some(a) = a else {
        return Some("this repository declares no [architecture] table in .coderipper.toml, so ARC-003 has no stable component and no volatile detail to compare; a missing declaration is a gap, not a pass (docs/rules/arc.md)".into());
    };
    let table_empty = a
        .volatile
        .as_ref()
        .is_none_or(|v| v.names.is_empty() && v.namespaces.is_empty() && !v.feature_cfg_in_policy);
    if table_empty {
        return Some("the [architecture] table has no [architecture.volatile] names, namespaces or feature_cfg_in_policy, so there is nothing to look for in stable components (docs/rules/arc.md)".into());
    }
    if !a.component.iter().any(is_stable_policy) {
        return Some("no [[architecture.component]] is tagged volatility = \"stable\" (with role \"policy\" or no role), so there is no stable policy to judge (docs/rules/arc.md)".into());
    }
    None
}

/// A component this rule judges: tagged stable, and not declared an adapter, infrastructure, composition or test support.
fn is_stable_policy(c: &architecture::ComponentDecl) -> bool {
    c.volatility == Some(Volatility::Stable) && matches!(c.role, None | Some(Role::Policy))
}

impl Check for VolatileDetailCheck {
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
        let graph = read_graph(&ctx.project_root)?;
        let names = graph.members.iter().map(|m| m.name.clone()).collect();
        arch.validate(&names, &file)?;
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = Resolved::new(&arch, &graph.members);
        let mut tallies = Vec::new();
        let mut notes = Vec::new();
        let volatile = arch.volatile.clone().unwrap_or_default();
        for c in arch.component.iter().filter(|c| is_stable_policy(c)) {
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
            let mut files: Vec<SourceFile> = Vec::new();
            let mut manifest = None;
            let mut crates = 0usize;
            for m in &graph.members {
                if resolved
                    .component
                    .get(m.name.as_str())
                    .and_then(Option::as_deref)
                    != Some(c.name.as_str())
                {
                    continue;
                }
                crates += 1;
                manifest.get_or_insert_with(|| m.manifest.clone());
                let dir = ctx.project_root.join(&m.dir);
                for mut f in read_sources(&dir)? {
                    if !m.dir.is_empty() {
                        f.path = format!("{}/{}", m.dir, f.path);
                    }
                    files.push(f);
                }
            }
            if crates == 0 {
                notes.push(format!(
                    "component `{}` matches no workspace member (ARC-001 reports the stale declaration); nothing to count",
                    c.name
                ));
                continue;
            }
            let counts = count(&files, &volatile)?;
            notes.push(format!(
                "stable component `{}`: {} crate(s), {} source file(s) read",
                c.name,
                crates,
                files.len()
            ));
            tallies.push((c.name.clone(), manifest, counts));
        }
        Ok(judge(tallies, &volatile, &project, notes))
    }
}

/// What was counted in one component.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Uses per declared volatile name.
    pub names: BTreeMap<String, usize>,
    /// Uses per declared namespace prefix.
    pub namespaces: BTreeMap<String, usize>,
    /// Feature-testing `cfg` attributes, per feature.
    pub features: BTreeMap<String, usize>,
    /// Feature-testing `cfg` attributes in all (an attribute naming two features counts once here).
    pub cfg_lines: usize,
    /// Uses per file (names, namespaces and cfg attributes together).
    pub per_file: BTreeMap<String, usize>,
}

impl Counts {
    fn uses(&self) -> usize {
        self.names.values().sum::<usize>() + self.namespaces.values().sum::<usize>()
    }

    fn merge(&mut self, other: Counts) {
        for (k, v) in other.names {
            *self.names.entry(k).or_default() += v;
        }
        for (k, v) in other.namespaces {
            *self.namespaces.entry(k).or_default() += v;
        }
        for (k, v) in other.features {
            *self.features.entry(k).or_default() += v;
        }
        self.cfg_lines += other.cfg_lines;
        for (k, v) in other.per_file {
            *self.per_file.entry(k).or_default() += v;
        }
    }
}

/// Counts the volatile detail in `files`. A file `syn` cannot parse is an error, never skipped: a smaller population reads as good news.
pub(crate) fn count(files: &[SourceFile], volatile: &VolatileDecl) -> anyhow::Result<Counts> {
    let mut total = Counts::default();
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        let mut w = Walk {
            volatile,
            counts: Counts::default(),
            in_item: false,
        };
        w.visit_file(&parsed);
        let n = w.counts.names.values().sum::<usize>()
            + w.counts.namespaces.values().sum::<usize>()
            + w.counts.cfg_lines;
        if n > 0 {
            w.counts.per_file.insert(f.path.clone(), n);
        }
        total.merge(w.counts);
    }
    Ok(total)
}

struct Walk<'a> {
    volatile: &'a VolatileDecl,
    counts: Counts,
    /// Inside an item whose tokens were already counted: a nested item adds attributes only, never names again.
    in_item: bool,
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item(&mut self, i: &'ast syn::Item) {
        let attrs: &[syn::Attribute] = match i {
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            syn::Item::Enum(x) => &x.attrs,
            syn::Item::Use(x) => &x.attrs,
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            syn::Item::Trait(x) => &x.attrs,
            syn::Item::Type(x) => &x.attrs,
            _ => &[],
        };
        if is_cfg_test(attrs) {
            return; // nothing inside a test item is counted
        }
        // an inline module is only a container: its items are counted one by one
        let container = matches!(i, syn::Item::Mod(m) if m.content.is_some());
        if !container && !self.in_item {
            self.count_tokens(i.to_token_stream());
            self.in_item = true;
            syn::visit::visit_item(self, i);
            self.in_item = false;
        } else {
            syn::visit::visit_item(self, i);
        }
    }

    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if !self.volatile.feature_cfg_in_policy {
            return;
        }
        if !(a.path().is_ident("cfg") || a.path().is_ident("cfg_attr")) {
            return;
        }
        let syn::Meta::List(list) = &a.meta else {
            return;
        };
        let mut features = Vec::new();
        collect_features(list.tokens.clone(), &mut features);
        if features.is_empty() {
            return;
        }
        self.counts.cfg_lines += 1;
        for f in features {
            *self.counts.features.entry(f).or_default() += 1;
        }
    }
}

impl Walk<'_> {
    fn count_tokens(&mut self, ts: TokenStream) {
        let mut flat = Vec::new();
        flatten(ts, &mut flat);
        for (i, t) in flat.iter().enumerate() {
            if let Tok::Ident(id) = t {
                if self.volatile.names.iter().any(|n| n == id) {
                    *self.counts.names.entry(id.clone()).or_default() += 1;
                }
            }
            for ns in &self.volatile.namespaces {
                let pattern = path_tokens(ns);
                if !pattern.is_empty() && flat[i..].starts_with(&pattern) {
                    *self.counts.namespaces.entry(ns.clone()).or_default() += 1;
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Ident(String),
    /// `::`
    Sep,
    /// Anything else, kept so that `a` `,` `::` is not read as a path.
    Other,
}

/// `"sqlx::"` -> `[Ident(sqlx), Sep]`; `"tokio::net::"` -> `[Ident(tokio), Sep, Ident(net), Sep]`.
fn path_tokens(ns: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    for seg in ns.split("::") {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        out.push(Tok::Ident(seg.to_string()));
        out.push(Tok::Sep);
    }
    out
}

fn flatten(ts: TokenStream, out: &mut Vec<Tok>) {
    let mut prev_colon_joint = false;
    for tt in ts {
        match tt {
            TokenTree::Group(g) => {
                prev_colon_joint = false;
                out.push(Tok::Other);
                flatten(g.stream(), out);
                out.push(Tok::Other);
            }
            TokenTree::Ident(i) => {
                prev_colon_joint = false;
                out.push(Tok::Ident(i.to_string()));
            }
            TokenTree::Punct(p) if p.as_char() == ':' => {
                if prev_colon_joint {
                    out.push(Tok::Sep);
                    prev_colon_joint = false;
                } else if p.spacing() == proc_macro2::Spacing::Joint {
                    prev_colon_joint = true;
                } else {
                    out.push(Tok::Other);
                }
            }
            _ => {
                prev_colon_joint = false;
                out.push(Tok::Other);
            }
        }
    }
}

/// The feature names in `feature = "name"` pairs anywhere inside a `cfg` attribute's tokens.
fn collect_features(ts: TokenStream, out: &mut Vec<String>) {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    for (i, t) in toks.iter().enumerate() {
        match t {
            TokenTree::Group(g) => collect_features(g.stream(), out),
            TokenTree::Ident(id) if id == "feature" => {
                if let (Some(TokenTree::Punct(eq)), Some(TokenTree::Literal(lit))) =
                    (toks.get(i + 1), toks.get(i + 2))
                {
                    if eq.as_char() == '=' {
                        out.push(lit.to_string().trim_matches('"').to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

fn top(map: &BTreeMap<String, usize>, n: usize) -> String {
    let mut v: Vec<(&String, &usize)> = map.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    v.iter()
        .take(n)
        .map(|(k, c)| format!("`{k}` x{c}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One finding per component that has anything to report, and a ranking note.
pub(crate) fn judge(
    tallies: Vec<(String, Option<String>, Counts)>,
    volatile: &VolatileDecl,
    project: &str,
    mut notes: Vec<String>,
) -> (Vec<Finding>, Vec<String>) {
    let mut findings = Vec::new();
    let mut ranking: Vec<(&str, usize, usize)> = Vec::new();
    for (name, manifest, c) in &tallies {
        ranking.push((name.as_str(), c.uses(), c.cfg_lines));
        if c.uses() == 0 && c.cfg_lines == 0 {
            continue;
        }
        let mut parts = Vec::new();
        if !c.names.is_empty() {
            parts.push(format!(
                "{} use(s) of the declared volatile names ({})",
                c.names.values().sum::<usize>(),
                top(&c.names, 5)
            ));
        }
        if !c.namespaces.is_empty() {
            parts.push(format!(
                "{} use(s) of the declared volatile namespaces ({})",
                c.namespaces.values().sum::<usize>(),
                top(&c.namespaces, 5)
            ));
        }
        if c.cfg_lines > 0 {
            parts.push(format!(
                "{} `cfg` attribute(s) that test a Cargo feature ({})",
                c.cfg_lines,
                top(&c.features, 5)
            ));
        }
        let severity = if c.uses() > 0 {
            Severity::Low
        } else {
            Severity::Info
        };
        findings.push(Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity,
            confidence: Confidence::Medium,
            project: project.into(),
            location: manifest.clone().map(|file| Location { file, line: None }),
            subject: Some(name.clone()),
            summary: format!(
                "the stable component `{name}` names volatile detail: {}",
                parts.join("; ")
            ),
            detail: format!(
                "The component is declared `volatility = \"stable\"`, so what changes often (the names {:?}, the namespaces {:?}{}) should depend on it, not appear in it. Counted in `{name}`: {}. Most affected files: {}. The count is syntactic (an identifier equal to a declared name is counted, a type alias or a macro's output is not seen) and each `use` and each later use of the imported name are separate places. Introduce a boundary the policy owns and let the volatile component implement it, or, if the use is deliberate, allowlist it with the component as the subject and a reason.",
                volatile.names,
                volatile.namespaces,
                if volatile.feature_cfg_in_policy { ", and build features" } else { "" },
                parts.join("; "),
                {
                    let t = top(&c.per_file, 3);
                    if t.is_empty() { "none".to_string() } else { t }
                }
            ),
            member: None,
            positive_control: None,
        });
    }
    findings.sort_by(|a, b| a.subject.cmp(&b.subject));
    ranking.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)).then(a.0.cmp(b.0)));
    if !ranking.is_empty() {
        notes.push(format!(
            "ARC-003 ranking of stable components by volatile uses (names + namespaces) and feature cfg attributes: {}",
            ranking
                .iter()
                .map(|(n, u, c)| format!("{n} {u}/{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    (findings, notes)
}

#[cfg(test)]
mod tests;
