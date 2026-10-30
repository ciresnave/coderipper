// SPDX-License-Identifier: MIT OR Apache-2.0
//! Qualified names for the items of one source file, so two same-named items in one file (`S::new` and `T::new`) stay distinct
//! as an allowlist `subject`. A name is its inline `mod {}` blocks, the impl's type (last path segment; generics dropped), the
//! trait, and enclosing functions, then the item: `m::S::method`. The scheme is the one `unused-parameters` already uses.
//!
//! Lookup is by the line and name rustc (or a tag) reports. Several candidates on one line with different qualifications are
//! ambiguous; [`Qualifier::name`] then returns the bare name, never a guess.

use syn::visit::Visit;

/// The qualified names of one file's items, keyed by the line of the item's identifier.
pub(crate) struct Qualifier {
    items: Vec<(u32, String, String)>,
}

impl Qualifier {
    /// Reads `source`; a file `syn` cannot parse yields an empty index, so every lookup falls back to the bare name.
    pub(crate) fn new(source: &str) -> Self {
        let mut v = Collect {
            scope: Vec::new(),
            out: Vec::new(),
        };
        if let Ok(file) = syn::parse_file(source) {
            v.visit_file(&file);
        }
        Self { items: v.out }
    }

    /// The qualified name of the item `name` declared on `line`, or `name` itself when it is not found or ambiguous.
    pub(crate) fn name(&self, line: u32, name: &str) -> String {
        let mut found = self
            .items
            .iter()
            .filter(|(l, n, _)| *l == line && n == name)
            .map(|(_, _, q)| q);
        match (found.next(), found.next()) {
            (Some(q), None) => q.clone(),
            _ => name.to_string(),
        }
    }
}

struct Collect {
    scope: Vec<String>,
    out: Vec<(u32, String, String)>,
}

impl Collect {
    fn item(&mut self, ident: &syn::Ident) {
        let mut q = self.scope.clone();
        q.push(ident.to_string());
        self.out.push((
            ident.span().start().line as u32,
            ident.to_string(),
            q.join("::"),
        ));
    }

    fn within(&mut self, name: String, body: impl FnOnce(&mut Self)) {
        self.scope.push(name);
        body(self);
        self.scope.pop();
    }
}

impl<'ast> Visit<'ast> for Collect {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        self.item(&m.ident);
        self.within(m.ident.to_string(), |s| syn::visit::visit_item_mod(s, m));
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
        self.within(ty, |s| syn::visit::visit_item_impl(s, i));
    }

    fn visit_item_trait(&mut self, t: &'ast syn::ItemTrait) {
        self.item(&t.ident);
        self.within(t.ident.to_string(), |s| syn::visit::visit_item_trait(s, t));
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.item(&f.sig.ident);
        self.within(f.sig.ident.to_string(), |s| syn::visit::visit_item_fn(s, f));
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.item(&f.sig.ident);
        self.within(f.sig.ident.to_string(), |s| {
            syn::visit::visit_impl_item_fn(s, f)
        });
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.item(&f.sig.ident);
        self.within(f.sig.ident.to_string(), |s| {
            syn::visit::visit_trait_item_fn(s, f)
        });
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.item(&i.ident);
        self.within(i.ident.to_string(), |s| syn::visit::visit_item_struct(s, i));
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.item(&i.ident);
        self.within(i.ident.to_string(), |s| syn::visit::visit_item_enum(s, i));
    }

    fn visit_variant(&mut self, v: &'ast syn::Variant) {
        self.item(&v.ident);
        self.within(v.ident.to_string(), |s| syn::visit::visit_variant(s, v));
    }

    fn visit_field(&mut self, f: &'ast syn::Field) {
        if let Some(ident) = &f.ident {
            self.item(ident);
        }
        syn::visit::visit_field(self, f);
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.item(&i.ident);
        syn::visit::visit_item_const(self, i);
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.item(&i.ident);
        syn::visit::visit_item_static(self, i);
    }

    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.item(&i.ident);
        syn::visit::visit_item_type(self, i);
    }

    fn visit_impl_item_const(&mut self, i: &'ast syn::ImplItemConst) {
        self.item(&i.ident);
        syn::visit::visit_impl_item_const(self, i);
    }

    fn visit_impl_item_type(&mut self, i: &'ast syn::ImplItemType) {
        self.item(&i.ident);
        syn::visit::visit_impl_item_type(self, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_named_methods_in_one_file_get_distinct_names() {
        let src =
            "struct S;\nstruct T;\nimpl S {\n    fn new() {}\n}\nimpl T {\n    fn new() {}\n}\n";
        let q = Qualifier::new(src);
        assert_eq!(q.name(4, "new"), "S::new");
        assert_eq!(q.name(7, "new"), "T::new");
    }

    #[test]
    fn modules_and_enclosing_functions_qualify() {
        let src = "mod a {\n    pub fn f() {\n        fn inner() {}\n    }\n}\n";
        let q = Qualifier::new(src);
        assert_eq!(q.name(2, "f"), "a::f");
        assert_eq!(q.name(3, "inner"), "a::f::inner");
    }

    #[test]
    fn fields_and_variants_are_qualified_by_their_type() {
        let src = "struct S {\n    x: i32,\n}\nenum E {\n    A,\n}\n";
        let q = Qualifier::new(src);
        assert_eq!(q.name(2, "x"), "S::x");
        assert_eq!(q.name(5, "A"), "E::A");
    }

    #[test]
    fn a_top_level_item_keeps_its_bare_name_and_an_unknown_one_falls_back() {
        let q = Qualifier::new("fn f() {}\n");
        assert_eq!(q.name(1, "f"), "f");
        assert_eq!(q.name(9, "g"), "g");
        assert_eq!(Qualifier::new("fn (").name(1, "f"), "f");
    }

    #[test]
    fn two_candidates_on_one_line_are_ambiguous_so_the_bare_name_stays() {
        let q = Qualifier::new(
            "mod a { fn f() {} }\nmod b { fn f() {} }\nmod c { fn g() {} fn g2() {} }\n",
        );
        assert_eq!(q.name(1, "f"), "a::f");
        let q = Qualifier::new("mod a { fn f() {} } mod b { fn f() {} }\n");
        assert_eq!(q.name(1, "f"), "f");
    }
}
