// SPDX-License-Identifier: MIT OR Apache-2.0
//! The facts `MOD-004` clusters: for each struct, which `self` fields and which sibling methods each of its inherent methods uses.
//! Read from `syn`'s tree, syntactically: no name is resolved by the compiler.
//!
//! **What it cannot see** (said in every finding): a field reached other than as `self.name` (a destructuring `let Self { a, .. } =
//! self`, a field passed on through a helper that takes `&self.a`); a method that only the type's callers use; items a macro
//! generates; a macro's body is read as tokens (`self` `.` name), so a macro that renames `self` is not followed; a type name two
//! structs in a crate share (noted, attributed to neither); a method in a trait `impl` (its signature is dictated by the trait, so it
//! says little about the type's own jobs).

use crate::checks::rust_sources::SourceFile;
use crate::checks::size_coupling::is_cfg_test;
use proc_macro2::{TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use syn::spanned::Spanned;
use syn::visit::Visit;

/// One inherent method that takes `self`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Method {
    pub name: String,
    /// The fields it reads or writes as `self.name`.
    pub fields: BTreeSet<String>,
    /// The sibling methods it calls as `self.name(..)`.
    pub calls: BTreeSet<String>,
    /// A body of one statement that touches at most one field and calls nothing: an accessor or a setter, which says nothing about
    /// what the type's jobs are.
    pub trivial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeFacts {
    pub name: String,
    pub file: String,
    /// 1-based line of the `struct` item.
    pub line: usize,
    pub fields: Vec<String>,
    pub methods: Vec<Method>,
}

/// One cluster: methods that share state, directly or through the methods they call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cluster {
    pub fields: BTreeSet<String>,
    pub methods: Vec<String>,
}

/// Every struct with its methods, across the crate's files. A name two structs share is left out and noted.
pub(crate) fn collect(files: &[SourceFile]) -> anyhow::Result<(Vec<TypeFacts>, Vec<String>)> {
    let mut structs: BTreeMap<String, Vec<TypeFacts>> = BTreeMap::new();
    let mut impls: BTreeMap<String, Vec<Method>> = BTreeMap::new();
    for f in files {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!("could not parse {} with syn ({e}); refusing to guess", f.path)
        })?;
        let mut w = Walk {
            file: &f.path,
            structs: Vec::new(),
            impls: Vec::new(),
        };
        w.visit_file(&parsed);
        for s in w.structs {
            structs.entry(s.name.clone()).or_default().push(s);
        }
        for (n, m) in w.impls {
            impls.entry(n).or_default().extend(m);
        }
    }
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for (name, defs) in structs {
        if defs.len() > 1 {
            notes.push(format!(
                "type name `{name}` is declared by {} structs in this crate, so its methods are attributed to none of them",
                defs.len()
            ));
            continue;
        }
        let mut t = defs.into_iter().next().expect("one definition");
        t.methods = impls.remove(&name).unwrap_or_default();
        out.push(t);
    }
    Ok((out, notes))
}

/// The clusters of a type's methods. Trivial methods and methods that touch no state are not clustered; a method's fields include the
/// fields of every sibling it calls, transitively, so a caller and its callee are one cluster.
pub(crate) fn clusters(t: &TypeFacts) -> Vec<Cluster> {
    let by_name: BTreeMap<&str, &Method> = t.methods.iter().map(|m| (m.name.as_str(), m)).collect();
    let effective = |m: &Method| -> BTreeSet<String> {
        let mut fields = m.fields.clone();
        let mut seen: BTreeSet<&str> = BTreeSet::from([m.name.as_str()]);
        let mut todo: Vec<&str> = m.calls.iter().map(String::as_str).collect();
        while let Some(c) = todo.pop() {
            if !seen.insert(c) {
                continue;
            }
            if let Some(callee) = by_name.get(c) {
                fields.extend(callee.fields.iter().cloned());
                todo.extend(callee.calls.iter().map(String::as_str));
            }
        }
        fields
    };
    let nodes: Vec<(&Method, BTreeSet<String>)> = t
        .methods
        .iter()
        .filter(|m| !m.trivial)
        .map(|m| (m, effective(m)))
        .filter(|(_, f)| !f.is_empty())
        .collect();
    // union-find over the methods, joined by a shared field
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn find(p: &mut Vec<usize>, i: usize) -> usize {
        if p[i] != i {
            let r = find(p, p[i]);
            p[i] = r;
        }
        p[i]
    }
    let mut owner: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, (_, fields)) in nodes.iter().enumerate() {
        for f in fields {
            match owner.get(f.as_str()) {
                Some(&j) => {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    parent[a] = b;
                }
                None => {
                    owner.insert(f, i);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Cluster> = BTreeMap::new();
    for i in 0..nodes.len() {
        let r = find(&mut parent, i);
        let g = groups.entry(r).or_insert_with(|| Cluster {
            fields: BTreeSet::new(),
            methods: Vec::new(),
        });
        g.fields.extend(nodes[i].1.iter().cloned());
        g.methods.push(nodes[i].0.name.clone());
    }
    groups.into_values().collect()
}

struct Walk<'a> {
    file: &'a str,
    structs: Vec<TypeFacts>,
    impls: Vec<(String, Vec<Method>)>,
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item(&mut self, i: &'ast syn::Item) {
        let attrs: &[syn::Attribute] = match i {
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            _ => &[],
        };
        if is_cfg_test(attrs) {
            return; // nothing inside a test item is measured
        }
        syn::visit::visit_item(self, i);
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        let fields = match &i.fields {
            syn::Fields::Named(f) => f
                .named
                .iter()
                .filter_map(|f| f.ident.as_ref().map(ToString::to_string))
                .collect(),
            syn::Fields::Unnamed(f) => (0..f.unnamed.len()).map(|n| n.to_string()).collect(),
            syn::Fields::Unit => Vec::new(),
        };
        self.structs.push(TypeFacts {
            name: i.ident.to_string(),
            file: self.file.to_string(),
            line: i.span().start().line,
            fields,
            methods: Vec::new(),
        });
        syn::visit::visit_item_struct(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_none() {
            if let syn::Type::Path(tp) = &*i.self_ty {
                if let Some(seg) = tp.path.segments.last() {
                    let methods = i
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            syn::ImplItem::Fn(f)
                                if f.sig.receiver().is_some() && !is_cfg_test(&f.attrs) =>
                            {
                                Some(method_of(f))
                            }
                            _ => None,
                        })
                        .collect();
                    self.impls.push((seg.ident.to_string(), methods));
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

fn method_of(f: &syn::ImplItemFn) -> Method {
    let mut u = Uses::default();
    u.visit_block(&f.block);
    let trivial = f.block.stmts.len() == 1 && u.fields.len() <= 1 && u.calls.is_empty();
    Method {
        name: f.sig.ident.to_string(),
        fields: u.fields,
        calls: u.calls,
        trivial,
    }
}

#[derive(Default)]
struct Uses {
    fields: BTreeSet<String>,
    calls: BTreeSet<String>,
}

fn is_self(e: &syn::Expr) -> bool {
    matches!(e, syn::Expr::Path(p) if p.qself.is_none() && p.path.is_ident("self"))
}

impl<'ast> Visit<'ast> for Uses {
    fn visit_expr_field(&mut self, e: &'ast syn::ExprField) {
        if is_self(&e.base) {
            self.fields.insert(match &e.member {
                syn::Member::Named(n) => n.to_string(),
                syn::Member::Unnamed(i) => i.index.to_string(),
            });
        }
        syn::visit::visit_expr_field(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        if is_self(&e.receiver) {
            self.calls.insert(e.method.to_string());
        }
        syn::visit::visit_expr_method_call(self, e);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.tokens(m.tokens.clone());
    }
}

impl Uses {
    /// A macro's arguments are tokens, not expressions: read `self` `.` name, and `self` `.` name `(` as a call.
    fn tokens(&mut self, ts: TokenStream) {
        let toks: Vec<TokenTree> = ts.into_iter().collect();
        let mut i = 0;
        while i < toks.len() {
            match &toks[i] {
                TokenTree::Group(g) => self.tokens(g.stream()),
                TokenTree::Ident(id) if id == "self" => {
                    if let (Some(TokenTree::Punct(p)), Some(TokenTree::Ident(name))) =
                        (toks.get(i + 1), toks.get(i + 2))
                    {
                        if p.as_char() == '.' {
                            let call = matches!(toks.get(i + 3),
                                Some(TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
                            if call {
                                self.calls.insert(name.to_string());
                            } else {
                                self.fields.insert(name.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(src: &str) -> Vec<TypeFacts> {
        collect(&[SourceFile {
            path: "src/lib.rs".into(),
            text: src.into(),
        }])
        .unwrap()
        .0
    }

    fn cluster_names(src: &str) -> Vec<Vec<String>> {
        let mut c: Vec<Vec<String>> = clusters(&types(src)[0])
            .into_iter()
            .map(|c| {
                let mut m = c.methods;
                m.sort();
                m
            })
            .collect();
        c.sort();
        c
    }

    const TWO_JOBS: &str = "pub struct S { a: u8, b: u8, c: u8, d: u8 }
impl S {
    fn x1(&self) -> u8 { self.a + self.b }
    fn x2(&mut self) { self.a = 1; self.b = 2; }
    fn y1(&self) -> u8 { self.c + self.d }
    fn y2(&mut self) { self.c = 3; self.d = 4; }
}";

    #[test]
    fn methods_over_disjoint_fields_are_two_clusters() {
        assert_eq!(
            cluster_names(TWO_JOBS),
            vec![vec!["x1", "x2"], vec!["y1", "y2"]]
        );
    }

    #[test]
    fn one_method_touching_both_groups_joins_them() {
        let src = TWO_JOBS.replace("}\n}", "}\n    fn bridge(&self) -> u8 { self.a + self.c }\n}");
        assert_eq!(cluster_names(&src).len(), 1, "{src}");
    }

    #[test]
    fn a_call_to_a_sibling_joins_the_caller_to_the_callees_fields() {
        let src = "pub struct S { a: u8, b: u8 }
impl S {
    fn helper(&self) -> u8 { let t = self.a; t + self.a }
    fn user(&self) -> u8 { let v = self.helper(); v + 1 }
    fn other(&self) -> u8 { let w = self.b; w + self.b }
}";
        assert_eq!(cluster_names(src), vec![vec!["helper", "user"], vec!["other"]]);
    }

    #[test]
    fn an_accessor_is_not_a_member_of_any_cluster() {
        let src = "pub struct S { a: u8 }
impl S { fn get_a(&self) -> u8 { self.a } fn set_a(&mut self, v: u8) { self.a = v; } }";
        assert!(cluster_names(src).is_empty());
    }

    #[test]
    fn a_field_used_inside_a_macro_is_seen() {
        let src = "pub struct S { a: u8, b: u8 }
impl S {
    fn show(&self) -> String { let s = format!(\"{} {}\", self.a, self.b); s }
}";
        let t = &types(src)[0];
        assert_eq!(t.methods[0].fields, BTreeSet::from(["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn a_method_call_inside_a_macro_is_a_call_not_a_field() {
        let src = "pub struct S { a: u8 }
impl S {
    fn f(&self) -> String { let s = format!(\"{}\", self.g()); s }
    fn g(&self) -> u8 { let t = self.a; t }
}";
        let t = &types(src)[0];
        assert!(t.methods[0].calls.contains("g") && t.methods[0].fields.is_empty());
    }

    #[test]
    fn trait_impl_methods_and_associated_functions_are_not_members() {
        let src = "pub struct S { a: u8 }
impl S { fn new() -> S { S { a: 0 } } }
impl Clone for S { fn clone(&self) -> S { S { a: self.a } } }";
        assert!(types(src)[0].methods.is_empty());
    }

    #[test]
    fn a_tuple_struct_field_is_its_index() {
        let src = "pub struct S(u8, u8);
impl S { fn f(&self) -> u8 { let a = self.0; a + self.1 } }";
        let t = &types(src)[0];
        assert_eq!(t.methods[0].fields, BTreeSet::from(["0".to_string(), "1".to_string()]));
    }

    #[test]
    fn test_items_are_not_read() {
        let src = "pub struct S { a: u8 }
impl S { fn f(&self) -> u8 { let a = self.a; a } }
#[cfg(test)]
impl S { fn g(&self) -> u8 { let a = self.a; a } }";
        assert_eq!(types(src)[0].methods.len(), 1);
    }

    #[test]
    fn a_name_two_structs_share_is_noted_and_left_out() {
        let files = [
            SourceFile { path: "src/a.rs".into(), text: "pub struct S;".into() },
            SourceFile { path: "src/b.rs".into(), text: "pub struct S;".into() },
        ];
        let (t, notes) = collect(&files).unwrap();
        assert!(t.is_empty() && notes.len() == 1);
    }

    #[test]
    fn a_file_syn_cannot_parse_is_an_error_not_a_skip() {
        let files = [SourceFile { path: "src/a.rs".into(), text: "fn (".into() }];
        assert!(collect(&files).is_err());
    }
}
