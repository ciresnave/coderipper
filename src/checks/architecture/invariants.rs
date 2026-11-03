// SPDX-License-Identifier: MIT OR Apache-2.0
//! Invariant-bearing types keep fields private (repository scope, Rust): can a caller build or change a value of a type that guards an
//! invariant without going through the check?
//!
//! Rule record: `MOD-010` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.8; the declaration is `invariant_types` on a component
//! (`docs/superpowers/specs/2026-11-01-architecture-declaration-proposal.md`).
//!
//! **Two halves.**
//! - **Declared (Low).** A type named in a component's `invariant_types` (found by name in the `src/` of the crates of that component)
//!   that has a `pub` field, or that derives `Default` or `Deserialize` while it has a validating-looking constructor: each of those
//!   builds a value that skipped the check. A declared name found in no crate of its component is an Info finding (a stale declaration).
//! - **Inferred (Info).** A `pub` struct with a **validating-looking constructor** and at least one `pub` field, in any workspace crate,
//!   declared or not: the constructor suggests there is something to guard, and the public field lets a caller go around it.
//!
//! **A validating-looking constructor** is an inherent function named `new`, `try_new`, `parse` or `from_*` that returns `Result<Self, _>`
//! (or the type's name) or `Option<Self>` and whose body can reject, i.e. names `Err`, `None`, `bail!`, `ensure!`, `assert!` or `?`.
//! A constructor that only wraps its arguments is not one.
//!
//! **Weak evidence by construction.** Syntactic: a type is matched by name (two types of one name in one crate share the facts); a
//! `pub(crate)` field is not public; `#[non_exhaustive]` is noted, not credited; a derive added by a macro, a manual `impl Default`
//! or `Deserialize`, a builder and an `unsafe` constructor are not seen; `#[serde(try_from = ..)]` and `#[serde(from = ..)]` count as a
//! validated path for `Deserialize`. A type with a public field and a checking constructor may be a plain data record whose check is a
//! convenience, which is why the inferred half is Info. The text names the fields and nothing else: it makes no change and offers no
//! options (patent watch, rows 64 and 65 of `docs/PATENT-SWEEP.md`).
//!
//! **No declaration:** the inferred half still runs; the note says the declared half needs `invariant_types`.
//!
//! **Known limitations / deliberate scope:** crate granularity for components; `src/` only, test items skipped; a file `syn` cannot parse
//! is an error, never a smaller count.

use super::{read_graph, Resolved};
use crate::architecture::{self, ArchitectureConfig};
use crate::check::{Check, CheckContext, Network, Scope, Unit};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_010;

/// Reports invariant-bearing types that a caller can build or change without the check (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct InvariantFieldsCheck;

impl InvariantFieldsCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for InvariantFieldsCheck {
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

    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let loaded = architecture::load(&ctx.project_root)?;
        let graph = read_graph(&ctx.project_root)?;
        if let Some((arch, file)) = &loaded {
            let names = graph.members.iter().map(|m| m.name.clone()).collect();
            arch.validate(&names, file)?;
        }
        let project = ctx.project_root.file_name().map_or_else(
            || "repository".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let resolved = loaded
            .as_ref()
            .map(|(a, _)| Resolved::new(a, &graph.members));
        let mut crates: Vec<CrateFacts> = Vec::new();
        for m in &graph.members {
            let files = read_sources(&ctx.project_root.join(&m.dir))?;
            let prefix = |p: &str| {
                if m.dir.is_empty() {
                    p.to_string()
                } else {
                    format!("{}/{p}", m.dir)
                }
            };
            let mut types = collect(&files, &prefix)?;
            types.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
            let component = resolved
                .as_ref()
                .and_then(|r| r.component.get(&m.name))
                .and_then(Clone::clone);
            crates.push(CrateFacts {
                name: m.name.clone(),
                component,
                types,
            });
        }
        let declared = loaded.as_ref().map(|(a, _)| declared_names(a));
        let (findings, mut notes) = judge(&crates, declared.as_ref(), &project);
        if declared.as_ref().is_none_or(BTreeMap::is_empty) {
            notes.push("no component declares `invariant_types`, so only the inferred half of MOD-010 ran (a public struct with a validating-looking constructor and a public field); declare the types that guard an invariant to have them judged at Low (docs/rules/mod.md)".into());
        }
        Ok((findings, notes))
    }
}

/// What the source says about one struct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TypeFacts {
    pub name: String,
    pub file: String,
    pub line: usize,
    /// Declared `pub`.
    pub public: bool,
    /// The `pub` fields (a tuple field by its index).
    pub pub_fields: Vec<String>,
    pub field_count: usize,
    pub derives: BTreeSet<String>,
    /// `#[serde(try_from = ..)]` or `#[serde(from = ..)]` is present.
    pub serde_validated: bool,
    pub non_exhaustive: bool,
    /// Inherent functions that look like validating constructors.
    pub constructors: Vec<String>,
}

pub(crate) struct CrateFacts {
    pub name: String,
    pub component: Option<String>,
    pub types: Vec<TypeFacts>,
}

/// Declared names by component: `component -> {type names}`.
fn declared_names(a: &ArchitectureConfig) -> BTreeMap<String, BTreeSet<String>> {
    a.component
        .iter()
        .filter(|c| !c.invariant_types.is_empty())
        .map(|c| (c.name.clone(), c.invariant_types.iter().cloned().collect()))
        .collect()
}

fn derive_names(attrs: &[syn::Attribute]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for a in attrs.iter().filter(|a| a.path().is_ident("derive")) {
        let _ = a.parse_nested_meta(|m| {
            if let Some(last) = m.path.segments.last() {
                out.insert(last.ident.to_string());
            }
            Ok(())
        });
    }
    out
}

fn serde_validated(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("serde"))
        .any(|a| {
            let mut found = false;
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("try_from") || m.path.is_ident("from") {
                    found = true;
                }
                // skip the value of `key = "value"` so the walk continues
                if m.input.peek(syn::Token![=]) {
                    let _: syn::Token![=] = m.input.parse()?;
                    let _: syn::Expr = m.input.parse()?;
                }
                Ok(())
            });
            found
        })
}

fn tokens_name_a_reject(ts: TokenStream) -> bool {
    ts.into_iter().any(|tt| match tt {
        TokenTree::Ident(i) => matches!(
            i.to_string().as_str(),
            "Err" | "None" | "bail" | "ensure" | "assert" | "assert_eq" | "assert_ne"
        ),
        TokenTree::Punct(p) => p.as_char() == '?',
        TokenTree::Group(g) => tokens_name_a_reject(g.stream()),
        _ => false,
    })
}

/// Whether the return type is `Result<Self, _>`, `Option<Self>` or one of them over the type's own name.
fn returns_self_or_nothing(ret: &syn::ReturnType, type_name: &str) -> bool {
    let syn::ReturnType::Type(_, ty) = ret else {
        return false;
    };
    let syn::Type::Path(p) = &**ty else {
        return false;
    };
    let Some(last) = p.path.segments.last() else {
        return false;
    };
    if last.ident != "Result" && last.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(syn::Type::Path(inner))) = args.args.first() else {
        return false;
    };
    inner
        .path
        .segments
        .last()
        .is_some_and(|s| s.ident == "Self" || s.ident == type_name)
}

fn is_constructor_name(n: &str) -> bool {
    matches!(n, "new" | "try_new" | "parse") || n.starts_with("from_")
}

/// The structs of the crate whose tracked sources are `files`, with their derives, public fields and constructors.
pub(crate) fn collect(
    files: &[SourceFile],
    prefix: &dyn Fn(&str) -> String,
) -> anyhow::Result<Vec<TypeFacts>> {
    let mut structs: Vec<TypeFacts> = Vec::new();
    let mut ctors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        walk(&parsed.items, &prefix(&f.path), &mut structs, &mut ctors);
    }
    for s in &mut structs {
        s.constructors = ctors.get(&s.name).cloned().unwrap_or_default();
    }
    Ok(structs)
}

fn walk(
    items: &[syn::Item],
    file: &str,
    structs: &mut Vec<TypeFacts>,
    ctors: &mut BTreeMap<String, Vec<String>>,
) {
    for item in items {
        match item {
            syn::Item::Struct(s) => {
                if is_cfg_test(&s.attrs) {
                    continue;
                }
                let mut pub_fields = Vec::new();
                for (i, field) in s.fields.iter().enumerate() {
                    if matches!(field.vis, syn::Visibility::Public(_)) {
                        pub_fields.push(
                            field
                                .ident
                                .as_ref()
                                .map_or_else(|| i.to_string(), ToString::to_string),
                        );
                    }
                }
                structs.push(TypeFacts {
                    name: s.ident.to_string(),
                    file: file.to_string(),
                    line: s.ident.span().start().line,
                    public: matches!(s.vis, syn::Visibility::Public(_)),
                    pub_fields,
                    field_count: s.fields.len(),
                    derives: derive_names(&s.attrs),
                    serde_validated: serde_validated(&s.attrs),
                    non_exhaustive: s.attrs.iter().any(|a| a.path().is_ident("non_exhaustive")),
                    constructors: Vec::new(),
                });
            }
            syn::Item::Impl(im) if im.trait_.is_none() && !is_cfg_test(&im.attrs) => {
                let syn::Type::Path(p) = &*im.self_ty else {
                    continue;
                };
                let Some(name) = p.path.segments.last().map(|s| s.ident.to_string()) else {
                    continue;
                };
                for it in &im.items {
                    let syn::ImplItem::Fn(f) = it else {
                        continue;
                    };
                    let fname = f.sig.ident.to_string();
                    if is_constructor_name(&fname)
                        && returns_self_or_nothing(&f.sig.output, &name)
                        && tokens_name_a_reject(f.block.to_token_stream())
                    {
                        ctors.entry(name.clone()).or_default().push(fname);
                    }
                }
            }
            syn::Item::Mod(m) => {
                if is_cfg_test(&m.attrs) {
                    continue;
                }
                if let Some((_, inner)) = &m.content {
                    walk(inner, file, structs, ctors);
                }
            }
            _ => {}
        }
    }
}

fn fields_list(t: &TypeFacts) -> String {
    t.pub_fields
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn finding(
    c: &CrateFacts,
    t: &TypeFacts,
    severity: Severity,
    confidence: Confidence,
    project: &str,
    summary: String,
    detail: String,
) -> Finding {
    Finding {
        check_id: CHECK_ID.into(),
        claim: Claim::Presence,
        severity,
        confidence,
        project: project.into(),
        location: Some(Location {
            file: t.file.clone(),
            line: u32::try_from(t.line).ok(),
        }),
        subject: Some(t.name.clone()),
        summary,
        detail,
        member: Some(c.name.clone()),
        positive_control: None,
    }
}

/// The findings and notes for the crates of a workspace.
pub(crate) fn judge(
    crates: &[CrateFacts],
    declared: Option<&BTreeMap<String, BTreeSet<String>>>,
    project: &str,
) -> (Vec<Finding>, Vec<String>) {
    let mut out = Vec::new();
    let mut declared_hit: BTreeSet<(String, String)> = BTreeSet::new();
    if let Some(d) = declared {
        for (comp, names) in d {
            for c in crates
                .iter()
                .filter(|c| c.component.as_deref() == Some(comp))
            {
                for t in &c.types {
                    if !names.contains(&t.name) {
                        continue;
                    }
                    declared_hit.insert((comp.clone(), t.name.clone()));
                    if !t.pub_fields.is_empty() {
                        out.push(finding(
                            c,
                            t,
                            Severity::Low,
                            Confidence::Medium,
                            project,
                            format!(
                                "the declared invariant-bearing type `{}` has {} public field(s) ({}): a caller can build or change a value without the check",
                                t.name,
                                t.pub_fields.len(),
                                fields_list(t)
                            ),
                            format!(
                                "`{}` is in `invariant_types` of the component `{comp}`, yet its field(s) {} are `pub`: any caller can write a struct literal or assign to them, so the invariant cannot be relied on anywhere. The constructor{} {}. Fields that are private and a constructor that checks keep the invariant in one place. Declared in the architecture, so Low; allowlist the type name with a reason if a field is deliberately open.",
                                t.name,
                                fields_list(t),
                                if t.constructors.len() == 1 { "" } else { "s" },
                                if t.constructors.is_empty() {
                                    "that checks was not found".to_string()
                                } else {
                                    format!("`{}` rejects bad input and can be bypassed", t.constructors.join("`, `"))
                                }
                            ),
                        ));
                    }
                    let bypass: Vec<&str> = ["Default", "Deserialize"]
                        .into_iter()
                        .filter(|d| t.derives.contains(*d))
                        .filter(|d| *d != "Deserialize" || !t.serde_validated)
                        .collect();
                    if !bypass.is_empty() && !t.constructors.is_empty() {
                        out.push(finding(
                            c,
                            t,
                            Severity::Info,
                            Confidence::Low,
                            project,
                            format!(
                                "the declared invariant-bearing type `{}` derives {}, which builds a value without its checking constructor",
                                t.name,
                                bypass.join(" and ")
                            ),
                            format!(
                                "`{}` has a constructor that can reject (`{}`), and also derives {}: the derive builds a value from defaults or from input without the check (`#[serde(try_from = ..)]` would route deserialisation through it). Syntactic: a manual `impl` of either is not seen.",
                                t.name,
                                t.constructors.join("`, `"),
                                bypass.join(" and ")
                            ),
                        ));
                    }
                }
            }
        }
        for (comp, names) in d {
            for n in names {
                if !declared_hit.contains(&(comp.clone(), n.clone())) {
                    out.push(Finding {
                        check_id: CHECK_ID.into(),
                        claim: Claim::Absence,
                        severity: Severity::Info,
                        confidence: Confidence::High,
                        project: project.into(),
                        location: None,
                        subject: Some(format!("{n}:stale")),
                        summary: format!(
                            "the declared invariant-bearing type `{n}` of the component `{comp}` is a struct in none of its crates"
                        ),
                        detail: format!(
                            "`invariant_types` of `{comp}` names `{n}`, but no struct of that name is in the `src/` of that component's crates (or the component matches no crate). The type was renamed, moved, is not a struct (an enum is not read) or the declaration is stale. Correct or remove the entry."
                        ),
                        member: None,
                        positive_control: Some(format!(
                            "the same read found {} struct(s) in {} crate(s)",
                            crates.iter().map(|c| c.types.len()).sum::<usize>(),
                            crates.len()
                        )),
                    });
                }
            }
        }
    }
    for c in crates {
        for t in &c.types {
            let is_declared = declared.is_some_and(|d| {
                c.component
                    .as_ref()
                    .and_then(|comp| d.get(comp))
                    .is_some_and(|names| names.contains(&t.name))
            });
            if is_declared || !t.public || t.pub_fields.is_empty() || t.constructors.is_empty() {
                continue;
            }
            out.push(finding(
                c,
                t,
                Severity::Info,
                Confidence::Low,
                project,
                format!(
                    "`{}` has a constructor that can reject (`{}`) and {} public field(s) ({}): a caller can go around the check",
                    t.name,
                    t.constructors.join("`, `"),
                    t.pub_fields.len(),
                    fields_list(t)
                ),
                format!(
                    "`{}` offers `{}`, which returns an error or nothing for bad input, while the field(s) {} are `pub`: a struct literal or an assignment skips the check{}. This may be a plain data record whose check is a convenience; if the type guards an invariant, make the fields private, or declare it in `invariant_types` to have it judged at Low. Inferred from the shape of the code, so Info.",
                    t.name,
                    t.constructors.join("`, `"),
                    fields_list(t),
                    if t.non_exhaustive {
                        " (`#[non_exhaustive]` stops a literal outside the crate, not an assignment)"
                    } else {
                        ""
                    }
                ),
            ));
        }
    }
    out.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b)).then_with(|| a.subject.cmp(&b.subject))
    });
    let structs: usize = crates.iter().map(|c| c.types.len()).sum();
    let notes = vec![format!(
        "MOD-010 read {structs} struct(s) in {} crate(s); {} finding(s)",
        crates.len(),
        out.len()
    )];
    (out, notes)
}

#[cfg(test)]
mod tests;
