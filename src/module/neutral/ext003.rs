// SPDX-License-Identifier: MIT OR Apache-2.0
//! EXT-003: a non-test trait implementation does not stub out part of its contract.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. In each `impl Trait for Type` block, a method
//! is reported when its body
//!
//! 1. calls `todo!` or `unimplemented!` (anywhere in the body, macro tokens included), or
//! 2. is short (at most three statements) and holds a string literal that says `not supported` or `not implemented` (any case), which
//!    is how an `Err(Error::Unsupported(".. not supported"))` or `bail!("not implemented")` refusal reads. The short-body limit is what
//!    keeps a long method that happens to log such a phrase out of the report.
//!
//! A trait method that has a default body in the trait definition is not a stub and is not looked at: only implementations are. `Drop`
//! implementations are skipped. One **Medium**-severity, Medium-confidence finding per stub, at the method.
//!
//! **What it cannot see:** a stub spelled another way (`panic!("..")`, `unreachable!()`, a method that returns an empty value and does
//! nothing: the rule's "silent empty override"), an error whose text does not use the two phrases, a stub in a macro-generated impl, and
//! whether the trait *meant* the method to be optional. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use proc_macro2::{TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::EXT_003;

/// The statements a refusing body may have at most.
const SHORT_BODY: usize = 3;

const PHRASES: &[&str] = &["not supported", "not implemented"];

const STUB_MACROS: &[&str] = &["todo", "unimplemented"];

#[derive(Default)]
struct Body {
    stub_macro: Option<&'static str>,
    phrase: bool,
}

impl Body {
    fn tokens(&mut self, ts: TokenStream) {
        for t in ts {
            match t {
                TokenTree::Group(g) => self.tokens(g.stream()),
                TokenTree::Literal(l) => {
                    let text = l.to_string().to_lowercase();
                    if text.starts_with(['"', 'r']) && PHRASES.iter().any(|p| text.contains(p)) {
                        self.phrase = true;
                    }
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Body {
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if let Some(seg) = m.path.segments.last() {
            let name = seg.ident.to_string();
            if let Some(found) = STUB_MACROS.iter().find(|n| **n == name) {
                self.stub_macro = Some(found);
            }
        }
        self.tokens(m.tokens.clone());
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        let text = s.value().to_lowercase();
        if PHRASES.iter().any(|p| text.contains(p)) {
            self.phrase = true;
        }
    }
}

struct Stub {
    line: u32,
    what: String,
    subject: String,
}

#[derive(Default)]
struct Walk {
    stubs: Vec<Stub>,
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map_or_else(|| "?".into(), |s| s.ident.to_string()),
        syn::Type::Reference(r) => type_name(&r.elem),
        _ => "?".into(),
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if let Some((path, _)) = &i.trait_ {
            let trait_name = path
                .segments
                .last()
                .map_or_else(String::new, |s| s.ident.to_string());
            if trait_name != "Drop" {
                let ty = type_name(&i.self_ty);
                for item in &i.items {
                    let syn::ImplItem::Fn(f) = item else {
                        continue;
                    };
                    if is_cfg_test(&f.attrs) {
                        continue;
                    }
                    let mut body = Body::default();
                    body.visit_block(&f.block);
                    let what = if let Some(m) = body.stub_macro {
                        Some(format!("calls `{m}!`"))
                    } else if body.phrase && f.block.stmts.len() <= SHORT_BODY {
                        Some(
                            "only reports that the operation is not supported or not implemented"
                                .to_string(),
                        )
                    } else {
                        None
                    };
                    if let Some(what) = what {
                        self.stubs.push(Stub {
                            line: u32::try_from(f.sig.span().start().line).unwrap_or(0),
                            what,
                            subject: format!("{ty}::{} (impl {trait_name})", f.sig.ident),
                        });
                    }
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for stub in walk.stubs {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!("`{}` is a stub: its body {}", stub.subject, stub.what),
                        "A client written against the trait expects this method to work. Implement it, or narrow the trait so this type is not asked to promise it (a separate trait, or a default body that says what the absence means).",
                    )
                    .location(Location::new(s.path.as_str(), Some(stub.line)))
                    .subject(stub.subject)
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that finds trait impl bodies"
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn todo_and_unimplemented_in_a_trait_impl_are_stubs() {
        let f = found("trait T { fn a(&self); fn b(&self); }\nstruct S;\nimpl T for S {\n    fn a(&self) { todo!() }\n    fn b(&self) { unimplemented!(\"later\") }\n}\n");
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
        assert_eq!(f[0].subject.as_deref(), Some("S::a (impl T)"));
        assert!(f[0].summary.contains("calls `todo!`"), "{}", f[0].summary);
    }

    #[test]
    fn a_short_refusal_is_a_stub_and_a_long_method_that_logs_the_phrase_is_not() {
        let src = "trait T { fn a(&self) -> Result<(), String>; fn b(&self) -> Result<(), String>; }\nstruct S;\nimpl T for S {\n    fn a(&self) -> Result<(), String> { Err(\"streaming is not supported\".into()) }\n    fn b(&self) -> Result<(), String> {\n        let x = 1;\n        let y = 2;\n        let z = 3;\n        log(\"fast path not supported, falling back\");\n        Ok(())\n    }\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].subject.as_deref().unwrap().starts_with("S::a"));
    }

    #[test]
    fn an_inherent_impl_a_default_body_a_drop_impl_and_test_code_are_not_looked_at() {
        let src = "trait T { fn d(&self) { todo!() } }\nstruct S;\nimpl S { fn a(&self) { todo!() } }\nimpl Drop for S { fn drop(&mut self) { unimplemented!() } }\n#[cfg(test)]\nmod t { struct U; impl super::T for U { fn d(&self) { todo!() } } }\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn a_refusal_through_a_macro_is_seen() {
        let f = found("trait T { fn a(&self) -> Result<(), E>; }\nstruct S;\nimpl T for S { fn a(&self) -> Result<(), E> { bail!(\"not implemented for this backend\") } }\n");
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn every_finding_validates() {
        for f in
            found("trait T { fn a(&self); }\nstruct S;\nimpl T for S { fn a(&self) { todo!() } }\n")
        {
            f.validate().unwrap();
        }
    }
}
