// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pass-through wrappers (project scope, Rust): which functions and methods do nothing but hand their arguments to one other call?
//!
//! Rule record: `MOD-008` in `rules/mod.toml`. Background: `docs/rules/mod.md`; the design is
//! `docs/superpowers/specs/2026-11-01-arc-mod-checks-design.md`, section 3.6.
//!
//! **What it reports.** A free function or an inherent method whose whole body is one call that forwards **all** of its parameters, in
//! order, unchanged: `fn f(a, b) -> R { g(a, b) }`, `fn f(&self, a) -> R { self.g(a) }` (an alias), `fn f(&self, a) -> R { g(&self.x, a) }`. The call
//! may be wrapped in `Ok(..)`, `Some(..)`, `?`, `.await` or `.into()`, and written as `return ..;`. The finding names the pair
//! (wrapper, callee). At least one parameter besides `self` must be forwarded: a function with none is delegation to a field or a
//! constant, which this rule does not call a wrapper. **A constructor is not a wrapper**: `Self(x)`, `Wrapper(x)` and `Self::Variant(x)` (a callee whose
//! last segment starts with a capital) build a value. **A call on a field is not a pass-through**: `fn add(&mut self, r: R) { self.rules.push(r) }` and
//! `fn query(&self, q: Q) -> R { self.storage.query(q) }` hide the field behind an interface (information hiding, a facade over a manager), so
//! they are not what this rule is for. On our own code a first version that counted them flagged 198 methods in one repository, nearly
//! all of which were documented facade or accessor methods of exactly that kind.
//!
//! **Exceptions that are kinds of item, not paths** (the design's list): methods of trait impls (the signature is dictated), and `pub`
//! functions written in the crate root file (`lib.rs`, `main.rs`), which are a facade. An item with an attribute other than `#[inline]`,
//! `#[must_use]`, `#[doc]` and `#[cfg(..)]`-free docs is not exempt but is not special either; test items are not read.
//!
//! **Weak evidence by construction (V1, low confidence).** Only the shape is checked. A wrapper may still add something the shape does
//! not show: a name that fixes a meaning, a narrower type that translates, a place to put an invariant later, a stable API over an unstable
//! callee. The text says so and the rule makes **no** change and offers no list of options (patent watch, rows 64 and 65 of
//! `docs/PATENT-SWEEP.md`).
//!
//! **Known limitations / deliberate scope:** syntactic and per file (the callee is named, not resolved); a macro body is not read;
//! generics, `where` clauses and `async`-ness of the callee are not compared; a wrapper whose parameters are patterns is not a pass-through.

use crate::check::{Check, CheckContext, Network, Scope};
use crate::checks::rust_sources::{read_sources, SourceFile};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Claim, Confidence, Finding, Location, Severity};
use quote::ToTokens;

/// The check's id, as `coderipper check` and allowlist entries name it.
pub const CHECK_ID: &str = crate::rule_id::MOD_008;

/// Reports functions that only forward their arguments to one other call (see the module docs).
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct PassThroughCheck;

impl PassThroughCheck {
    /// The check.
    pub fn new() -> Self {
        Self
    }
}

impl Check for PassThroughCheck {
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
        self.run_noted(ctx).map(|(findings, _)| findings)
    }

    fn run_noted(&self, ctx: &CheckContext) -> anyhow::Result<(Vec<Finding>, Vec<String>)> {
        let (package, _) = crate::package::locate(&ctx.project_root)?;
        let files = read_sources(&package.dir)?;
        let (wrappers, functions) = collect(&files)?;
        let project = package.dir.file_name().map_or_else(
            || package.name.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let notes = vec![format!(
            "MOD-008 read {functions} function(s) and method(s); {} pass-through wrapper(s)",
            wrappers.len()
        )];
        Ok((judge(&wrappers, &project), notes))
    }
}

/// One wrapper and what it forwards to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wrapper {
    /// `name` or `Type::name`.
    pub name: String,
    pub callee: String,
    pub file: String,
    pub line: usize,
    pub public: bool,
}

fn forwards_ident(e: &syn::Expr, name: &str) -> bool {
    matches!(e, syn::Expr::Path(p) if p.qself.is_none() && p.path.is_ident(name))
}

/// `self`, `self.a`, `self.a.b`, with `&` or `&mut` in front.
fn is_self_chain(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::Path(p) => p.path.is_ident("self"),
        syn::Expr::Field(f) => is_self_chain(&f.base),
        syn::Expr::Reference(r) => is_self_chain(&r.expr),
        syn::Expr::Paren(p) => is_self_chain(&p.expr),
        _ => false,
    }
}

/// Removes `Ok(..)`, `Some(..)`, `?`, `.await`, `.into()` and parentheses from around a call.
fn strip(e: &syn::Expr) -> &syn::Expr {
    match e {
        syn::Expr::Paren(p) => strip(&p.expr),
        syn::Expr::Group(g) => strip(&g.expr),
        syn::Expr::Try(t) => strip(&t.expr),
        syn::Expr::Await(a) => strip(&a.base),
        syn::Expr::Return(r) => r.expr.as_deref().map_or(e, strip),
        syn::Expr::MethodCall(m) if m.method == "into" && m.args.is_empty() => strip(&m.receiver),
        syn::Expr::Call(c) if c.args.len() == 1 => match &*c.func {
            syn::Expr::Path(p) if p.path.is_ident("Ok") || p.path.is_ident("Some") => {
                strip(&c.args[0])
            }
            _ => e,
        },
        _ => e,
    }
}

/// The callee of the single call in `block` when it forwards every parameter in `params`, in order.
fn forwarded_callee(block: &syn::Block, params: &[String], has_self: bool) -> Option<String> {
    let [stmt] = block.stmts.as_slice() else {
        return None;
    };
    let expr = match stmt {
        syn::Stmt::Expr(e, _) => e,
        _ => return None,
    };
    let core = strip(expr);
    match core {
        syn::Expr::Call(c) => {
            let syn::Expr::Path(fp) = &*c.func else {
                return None;
            };
            let args: Vec<&syn::Expr> = c.args.iter().collect();
            let rest: &[&syn::Expr] = if has_self && args.first().is_some_and(|a| is_self_chain(a))
            {
                &args[1..]
            } else {
                &args
            };
            // `Self(x)`, `Wrapper(x)` and `Self::Variant(x)` build a value: a constructor, not a layer over another function
            let builds_a_value = fp
                .path
                .segments
                .last()
                .is_some_and(|s| s.ident.to_string().starts_with(char::is_uppercase));
            (!builds_a_value
                && rest.len() == params.len()
                && rest.iter().zip(params).all(|(a, p)| forwards_ident(a, p)))
            .then(|| fp.path.to_token_stream().to_string().replace(' ', ""))
        }
        syn::Expr::MethodCall(m) => {
            // only an alias for another method of the same type: a call on a field (`self.inner.go(v)`) hides the field, which is
            // what a wrapper is for, so it is not one
            if !matches!(&*m.receiver, syn::Expr::Path(p) if p.path.is_ident("self")) {
                return None;
            }
            let args: Vec<&syn::Expr> = m.args.iter().collect();
            (args.len() == params.len()
                && args.iter().zip(params).all(|(a, p)| forwards_ident(a, p)))
            .then(|| {
                format!(
                    "{}.{}",
                    m.receiver.to_token_stream().to_string().replace(' ', ""),
                    m.method
                )
            })
        }
        _ => None,
    }
}

fn function_shape(sig: &syn::Signature, block: &syn::Block) -> Option<String> {
    let mut params = Vec::new();
    let mut has_self = false;
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(_) => has_self = true,
            syn::FnArg::Typed(pt) => match &*pt.pat {
                syn::Pat::Ident(pi) if pi.subpat.is_none() && pi.by_ref.is_none() => {
                    params.push(pi.ident.to_string());
                }
                _ => return None,
            },
        }
    }
    if params.is_empty() {
        return None;
    }
    forwarded_callee(block, &params, has_self)
}

/// The wrappers in `files`, and how many functions and methods were read.
pub(crate) fn collect(files: &[SourceFile]) -> anyhow::Result<(Vec<Wrapper>, usize)> {
    let mut wrappers = Vec::new();
    let mut functions = 0usize;
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        let root = f.path == "src/lib.rs" || f.path == "src/main.rs";
        walk(&parsed.items, &f.path, root, &mut wrappers, &mut functions);
    }
    wrappers.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok((wrappers, functions))
}

fn walk(
    items: &[syn::Item],
    file: &str,
    at_root: bool,
    wrappers: &mut Vec<Wrapper>,
    functions: &mut usize,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) if !is_cfg_test(&f.attrs) => {
                *functions += 1;
                let public = matches!(f.vis, syn::Visibility::Public(_));
                if public && at_root {
                    continue; // a facade
                }
                if let Some(callee) = function_shape(&f.sig, &f.block) {
                    wrappers.push(Wrapper {
                        name: f.sig.ident.to_string(),
                        callee,
                        file: file.to_string(),
                        line: f.sig.ident.span().start().line,
                        public,
                    });
                }
            }
            syn::Item::Impl(im) if im.trait_.is_none() && !is_cfg_test(&im.attrs) => {
                let ty = im.self_ty.to_token_stream().to_string().replace(' ', "");
                for it in &im.items {
                    if let syn::ImplItem::Fn(m) = it {
                        if is_cfg_test(&m.attrs) {
                            continue;
                        }
                        *functions += 1;
                        if let Some(callee) = function_shape(&m.sig, &m.block) {
                            wrappers.push(Wrapper {
                                name: format!("{ty}::{}", m.sig.ident),
                                callee,
                                file: file.to_string(),
                                line: m.sig.ident.span().start().line,
                                public: matches!(m.vis, syn::Visibility::Public(_)),
                            });
                        }
                    }
                }
            }
            syn::Item::Mod(m) if !is_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = &m.content {
                    walk(inner, file, false, wrappers, functions);
                }
            }
            _ => {}
        }
    }
}

/// One finding per wrapper.
pub(crate) fn judge(wrappers: &[Wrapper], project: &str) -> Vec<Finding> {
    wrappers
        .iter()
        .map(|w| Finding {
            check_id: CHECK_ID.into(),
            claim: Claim::Presence,
            severity: Severity::Info,
            confidence: Confidence::Low,
            project: project.into(),
            location: Some(Location {
                file: w.file.clone(),
                line: u32::try_from(w.line).ok(),
            }),
            subject: Some(w.name.clone()),
            summary: format!(
                "`{}` only passes its arguments on to `{}`",
                w.name, w.callee
            ),
            detail: format!(
                "The whole body of `{}`{} is one call to `{}` with the same arguments in the same order. A reader hops through a layer that hides nothing. It may still be worth keeping: a name that fixes a meaning, a narrower type, a place for an invariant later, a stable API over a callee that will change. This is the shape only, so Info at low confidence; allowlist the subject with the reason if the layer is deliberate.",
                w.name,
                if w.public { " (a `pub` item)" } else { "" },
                w.callee
            ),
            member: None,
            positive_control: None,
        })
        .collect()
}

#[cfg(test)]
mod tests;
