//! Finds every function parameter that rustc's `unused_variables` lint could report, with the exact
//! position rustc will report it at.
//!
//! rustc words an unused parameter exactly like an unused local ("unused variable: `x`"), so the
//! diagnostic alone cannot say which one it is. Joining it to the positions found here can.
//!
//! In scope: parameters of free functions (including nested ones) and of inherent-impl methods.
//! Out of scope, on purpose: the OWN signature of a trait method declaration, a trait default body or
//! a trait-impl method (the signature is dictated by the trait, so the parameter cannot simply be
//! removed), closure parameters, and anything inside a macro body (`syn` does not see inside one).
//! A function defined INSIDE one of those bodies is an ordinary free function and is in scope.

use syn::spanned::Spanned;
use syn::visit::Visit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamSite {
    /// Forward-slash path relative to the project root, e.g. `src/lib.rs`.
    pub file: String,
    /// 1-based line and 1-based CHARACTER column, the way rustc's JSON reports them.
    pub line: u32,
    pub column: u32,
    /// The parameter's name, e.g. `b`.
    pub name: String,
    /// Qualified function path: modules, the impl's type, enclosing functions, then the function,
    /// e.g. `m::S::method`. Qualified so two same-named functions in one file stay distinct.
    pub function: String,
}

impl ParamSite {
    /// The finding's fingerprint, e.g. `m::S::method::p`.
    pub fn subject(&self) -> String {
        format!("{}::{}", self.function, self.name)
    }
}

pub fn find_param_sites(file: &str, source: &str) -> anyhow::Result<Vec<ParamSite>> {
    let parsed = syn::parse_file(source)
        .map_err(|e| anyhow::anyhow!("could not parse {file} with syn ({e}); refusing to guess"))?;
    let mut visitor = Sites {
        file,
        scope: Vec::new(),
        out: Vec::new(),
        forced_signatures: false,
    };
    visitor.visit_file(&parsed);
    Ok(visitor.out)
}

struct Sites<'a> {
    file: &'a str,
    scope: Vec<String>,
    out: Vec<ParamSite>,
    /// True while inside a trait impl: its methods' own signatures are not ours to report.
    forced_signatures: bool,
}

impl Sites<'_> {
    fn collect(&mut self, sig: &syn::Signature) {
        let mut function = self.scope.clone();
        function.push(sig.ident.to_string());
        let function = function.join("::");
        for input in &sig.inputs {
            if let syn::FnArg::Typed(typed) = input {
                let mut idents = PatIdents(Vec::new());
                idents.visit_pat(&typed.pat);
                for (name, start) in idents.0 {
                    if name.starts_with('_') {
                        continue; // rustc exempts these by convention, so it never reports them
                    }
                    self.out.push(ParamSite {
                        file: self.file.to_string(),
                        line: start.line as u32,
                        column: start.column as u32 + 1,
                        name,
                        function: function.clone(),
                    });
                }
            }
        }
    }

    fn descend_into_body(&mut self, name: &syn::Ident, body: impl FnOnce(&mut Self)) {
        self.scope.push(name.to_string());
        body(self);
        self.scope.pop();
    }
}

impl<'ast> Visit<'ast> for Sites<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        self.descend_into_body(&m.ident, |this| syn::visit::visit_item_mod(this, m));
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let ty = match i.self_ty.as_ref() {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map_or_else(|| "<impl>".to_string(), |s| s.ident.to_string()),
            _ => "<impl>".to_string(),
        };
        // A trait impl's signatures are dictated by the trait; its method BODIES are still walked.
        let outer = std::mem::replace(&mut self.forced_signatures, i.trait_.is_some());
        self.scope.push(ty);
        syn::visit::visit_item_impl(self, i);
        self.scope.pop();
        self.forced_signatures = outer;
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        // Declarations and default bodies: the signature belongs to the trait. Default BODIES are
        // still walked (see `visit_trait_item_fn`).
        self.scope.push(t.ident.to_string());
        syn::visit::visit_item_trait(self, t);
        self.scope.pop();
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        if let Some(block) = &f.default {
            self.descend_into_body(&f.sig.ident, |this| this.visit_block(block));
        }
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.collect(&f.sig);
        self.descend_into_body(&f.sig.ident, |this| syn::visit::visit_item_fn(this, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if !self.forced_signatures {
            self.collect(&f.sig);
        }
        self.descend_into_body(&f.sig.ident, |this| syn::visit::visit_impl_item_fn(this, f));
    }
}

/// Every identifier bound by one parameter's pattern, with its start position. `PatIdent`'s span
/// starts at `mut` / `ref` when present, which is where rustc points.
struct PatIdents(Vec<(String, proc_macro2::LineColumn)>);

impl<'ast> Visit<'ast> for PatIdents {
    fn visit_pat_ident(&mut self, p: &'ast syn::PatIdent) {
        self.0.push((p.ident.to_string(), p.span().start()));
        syn::visit::visit_pat_ident(self, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sites(src: &str) -> Vec<(String, String, u32, u32)> {
        find_param_sites("src/lib.rs", src)
            .unwrap()
            .into_iter()
            .map(|s| (s.subject(), s.name, s.line, s.column))
            .collect()
    }

    fn subjects(src: &str) -> Vec<String> {
        sites(src).into_iter().map(|s| s.0).collect()
    }

    #[test]
    fn a_free_function_parameter_has_its_exact_position() {
        assert_eq!(
            sites("pub fn free(a: i32, b: i32) -> i32 { a }\n"),
            vec![
                ("free::a".into(), "a".into(), 1, 13),
                ("free::b".into(), "b".into(), 1, 21),
            ]
        );
    }

    #[test]
    fn an_inherent_method_is_qualified_by_its_type_and_self_is_not_a_parameter() {
        assert_eq!(
            subjects("struct S;\nimpl S {\n    fn m(&self, p: i32) {}\n}\n"),
            vec!["S::m::p"]
        );
    }

    #[test]
    fn modules_and_enclosing_functions_qualify_the_subject() {
        let src = "mod a {\n    pub mod b {\n        pub fn f(x: i32) {\n            fn inner(y: i32) {}\n        }\n    }\n}\n";
        assert_eq!(subjects(src), vec!["a::b::f::x", "a::b::f::inner::y"]);
    }

    #[test]
    fn same_named_methods_on_different_types_stay_distinct() {
        let src =
            "struct S; struct T;\nimpl S { fn new(a: i32) {} }\nimpl T { fn new(a: i32) {} }\n";
        assert_eq!(subjects(src), vec!["S::new::a", "T::new::a"]);
    }

    #[test]
    fn a_mut_binding_is_positioned_at_mut_and_patterns_yield_every_name() {
        assert_eq!(
            sites("fn f(mut m: i32, (a, b): (i32, i32)) {}\n"),
            vec![
                ("f::m".into(), "m".into(), 1, 6),
                ("f::a".into(), "a".into(), 1, 19),
                ("f::b".into(), "b".into(), 1, 22),
            ]
        );
    }

    #[test]
    fn underscore_prefixed_names_are_skipped_because_rustc_never_reports_them() {
        assert_eq!(subjects("fn f(_a: i32, b: i32, _: i32) {}\n"), vec!["f::b"]);
    }

    #[test]
    fn trait_declarations_default_bodies_trait_impls_and_closures_are_out_of_scope() {
        let src = "\
trait T {
    fn req(&self, r: i32);
    fn dflt(&self, d: i32) {}
}
struct S;
impl T for S {
    fn req(&self, r: i32) {}
}
fn uses_closure() { let _ = |e: i32| 1; }
";
        assert_eq!(subjects(src), Vec::<String>::new());
    }

    #[test]
    fn nested_functions_inside_trait_impl_methods_and_trait_default_bodies_are_in_scope() {
        // Review finding: the method's own signature is dictated by the trait, but a helper fn
        // defined INSIDE its body is an ordinary free function.
        let src = "\
struct S;
impl std::fmt::Display for S {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        fn helper(h: i32) {}
        Ok(())
    }
}
trait T {
    fn d(&self, x: i32) {
        fn hd(y: i32) {}
    }
}
";
        assert_eq!(subjects(src), vec!["S::fmt::helper::h", "T::d::hd::y"]);
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        // `é` is 2 bytes; rustc reports a 1-based character column.
        let found = sites("/* é */ fn f(a: i32) {}\n");
        assert_eq!(found, vec![("f::a".into(), "a".into(), 1, 14)]);
    }

    #[test]
    fn a_file_syn_cannot_parse_is_an_error_not_a_silent_skip() {
        assert!(find_param_sites("src/lib.rs", "this is not rust {{{").is_err());
    }
}
