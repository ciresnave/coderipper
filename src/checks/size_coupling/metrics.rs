// SPDX-License-Identifier: MIT OR Apache-2.0
//! The facts `MOD-005` compares: per file, its code lines outside test items, and the other modules it names (fan-out); per type, its
//! fields and inherent methods. Everything here is read from `syn`'s tree and is syntactic: no name is resolved by the compiler.
//!
//! **What it cannot see** (said in every finding): items a macro generates; a module that is not declared with `mod name;` in a file
//! this module reads; glob-import targets (`use a::*` names module `a`, not what it brings in); `#[path]` modules; a method added to
//! a type by an `impl` in another crate or inside a macro; a type name two modules both define (counted nowhere, noted).

use crate::checks::rust_sources::SourceFile;
use proc_macro2::LineColumn;
use std::collections::{BTreeMap, BTreeSet};
use syn::spanned::Spanned;
use syn::visit::Visit;

/// What was measured in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileMetrics {
    pub path: String,
    /// Lines that are neither blank, nor only a `//` comment, nor inside a test item.
    pub code_lines: usize,
    /// The other modules of this crate the file names, as `a::b` (the crate root is the empty string).
    pub fan_out: BTreeSet<String>,
    /// Distinct other files that name this file's module. Filled in by [`measure`].
    pub fan_in: usize,
    /// Types whose `struct` is declared in this file.
    pub types: Vec<TypeMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeMetrics {
    pub name: String,
    /// 1-based line of the `struct` keyword's item.
    pub line: usize,
    pub fields: usize,
    /// Methods in the type's inherent `impl` blocks (trait-impl methods are dictated by the trait and are not counted).
    pub methods: usize,
}

/// The module path a file declares, from its path under `src/`: `src/lib.rs` is the crate root, `src/a/mod.rs` is `a`, `src/a/b.rs`
/// is `a::b`. A file under `src/bin/` is its own crate root and is `None`: its `crate::` paths mean another crate.
pub(crate) fn module_of(path: &str) -> Option<Vec<String>> {
    let rest = path.strip_prefix("src/")?;
    if rest.starts_with("bin/") {
        return None;
    }
    let mut parts: Vec<&str> = rest.split('/').collect();
    let file = parts.pop()?;
    let stem = file.strip_suffix(".rs")?;
    let mut module: Vec<String> = parts.iter().map(|s| (*s).to_string()).collect();
    if !(stem == "mod" || (module.is_empty() && (stem == "lib" || stem == "main"))) {
        module.push(stem.to_string());
    }
    Some(module)
}

pub(crate) fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if a.path().is_ident("test") {
            return true;
        }
        if !a.path().is_ident("cfg") {
            return false;
        }
        let syn::Meta::List(list) = &a.meta else {
            return false;
        };
        let text = list.tokens.to_string().replace(' ', "");
        !text.contains("not(test)")
            && text
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == "test")
    })
}

/// Measures every file. A file `syn` cannot parse is an error, never skipped: a smaller population would read as good news.
pub(crate) fn measure(files: &[SourceFile]) -> anyhow::Result<(Vec<FileMetrics>, Vec<String>)> {
    let mut known: BTreeSet<Vec<String>> = BTreeSet::new();
    for f in files {
        if let Some(m) = module_of(&f.path) {
            known.insert(m);
        }
    }
    let mut out = Vec::new();
    let mut struct_homes: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new(); // name -> (file index, type index)
    let mut impl_methods: BTreeMap<String, usize> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        let parsed = syn::parse_file(&f.text).map_err(|e| {
            anyhow::anyhow!(
                "could not parse {} with syn ({e}); refusing to guess",
                f.path
            )
        })?;
        let module = module_of(&f.path);
        let mut v = Walk {
            module: module.clone().unwrap_or_default(),
            resolvable: module.is_some(),
            known: &known,
            test_spans: Vec::new(),
            fan_out: BTreeSet::new(),
            structs: Vec::new(),
            impls: Vec::new(),
        };
        v.visit_file(&parsed);
        let code_lines = count_code_lines(&f.text, &v.test_spans);
        let own = module.clone().unwrap_or_default().join("::");
        let mut fan_out = v.fan_out;
        fan_out.remove(&own);
        // a module naming one of its own ancestors is containment, not coupling out (design note section 3.2, ASK 2)
        if let Some(m) = &module {
            fan_out.retain(|t| {
                let t: Vec<&str> = if t.is_empty() {
                    Vec::new()
                } else {
                    t.split("::").collect()
                };
                !(t.len() < m.len() && m.iter().zip(&t).all(|(a, b)| a == b))
            });
        }
        let mut types = Vec::new();
        for (name, line, fields) in v.structs {
            struct_homes
                .entry(name.clone())
                .or_default()
                .push((i, types.len()));
            types.push(TypeMetrics {
                name,
                line,
                fields,
                methods: 0,
            });
        }
        for (name, n) in v.impls {
            *impl_methods.entry(name).or_default() += n;
        }
        out.push(FileMetrics {
            path: f.path.clone(),
            code_lines,
            fan_out,
            fan_in: 0,
            types,
        });
    }
    let mut notes = Vec::new();
    for (name, homes) in &struct_homes {
        if homes.len() > 1 {
            notes.push(format!(
                "type name `{name}` is declared by {} structs in this crate, so its methods are attributed to none of them",
                homes.len()
            ));
            continue;
        }
        let (fi, ti) = homes[0];
        out[fi].types[ti].methods = impl_methods.get(name).copied().unwrap_or(0);
    }
    // fan-in: distinct other files naming a module, counted per module and shown on the file that declares it
    let mut named_by: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for (i, m) in out.iter().enumerate() {
        for target in &m.fan_out {
            named_by.entry(target.clone()).or_default().insert(i);
        }
    }
    for (i, f) in files.iter().enumerate() {
        if let Some(m) = module_of(&f.path) {
            out[i].fan_in = named_by.get(&m.join("::")).map_or(0, BTreeSet::len);
            // a file's own module is not in its own fan-out, so it is not counted here
        }
    }
    Ok((out, notes))
}

fn count_code_lines(text: &str, test_spans: &[(usize, usize)]) -> usize {
    text.lines()
        .enumerate()
        .filter(|(n, line)| {
            let n = n + 1;
            let t = line.trim();
            !t.is_empty()
                && !t.starts_with("//")
                && !test_spans.iter().any(|(a, b)| n >= *a && n <= *b)
        })
        .count()
}

struct Walk<'a> {
    /// The module the visitor is in (file module plus inline `mod` blocks).
    module: Vec<String>,
    /// Whether `crate::`, `self::` and `super::` mean something here (not in a `src/bin` crate).
    resolvable: bool,
    known: &'a BTreeSet<Vec<String>>,
    test_spans: Vec<(usize, usize)>,
    fan_out: BTreeSet<String>,
    structs: Vec<(String, usize, usize)>,
    impls: Vec<(String, usize)>,
}

impl Walk<'_> {
    fn note_test(&mut self, start: LineColumn, end: LineColumn) {
        self.test_spans.push((start.line, end.line));
    }

    /// Resolve a path's segments to the longest known module of this crate, and record it.
    fn record_path(&mut self, segs: &[String]) {
        if !self.resolvable || segs.is_empty() {
            return;
        }
        let mut abs: Vec<String>;
        match segs[0].as_str() {
            "crate" => abs = segs[1..].to_vec(),
            "self" => {
                abs = self.module.clone();
                abs.extend_from_slice(&segs[1..]);
            }
            "super" => {
                abs = self.module.clone();
                let mut rest = segs;
                while rest.first().map(String::as_str) == Some("super") {
                    if abs.pop().is_none() {
                        return;
                    }
                    rest = &rest[1..];
                }
                abs.extend_from_slice(rest);
            }
            first => {
                // 2018 uniform paths: a child module of the current module named by its bare name
                let mut child = self.module.clone();
                child.push(first.to_string());
                if !self.known.contains(&child) {
                    return;
                }
                abs = self.module.clone();
                abs.extend_from_slice(segs);
            }
        }
        while !abs.is_empty() && !self.known.contains(&abs) {
            abs.pop();
        }
        if self.known.contains(&abs) {
            self.fan_out.insert(abs.join("::"));
        }
    }

    fn use_tree(&mut self, prefix: &mut Vec<String>, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.use_tree(prefix, &p.tree);
                prefix.pop();
            }
            syn::UseTree::Name(n) => {
                prefix.push(n.ident.to_string());
                self.record_path(&prefix.clone());
                prefix.pop();
            }
            syn::UseTree::Rename(r) => {
                prefix.push(r.ident.to_string());
                self.record_path(&prefix.clone());
                prefix.pop();
            }
            syn::UseTree::Glob(_) => self.record_path(&prefix.clone()),
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.use_tree(prefix, t);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item(&mut self, i: &'ast syn::Item) {
        let attrs: &[syn::Attribute] = match i {
            syn::Item::Fn(x) => &x.attrs,
            syn::Item::Mod(x) => &x.attrs,
            syn::Item::Impl(x) => &x.attrs,
            syn::Item::Struct(x) => &x.attrs,
            syn::Item::Enum(x) => &x.attrs,
            syn::Item::Trait(x) => &x.attrs,
            syn::Item::Const(x) => &x.attrs,
            syn::Item::Static(x) => &x.attrs,
            syn::Item::Use(x) => &x.attrs,
            syn::Item::Type(x) => &x.attrs,
            _ => &[],
        };
        if is_cfg_test(attrs) {
            let s = i.span();
            self.note_test(s.start(), s.end());
            return; // nothing inside a test item is measured
        }
        syn::visit::visit_item(self, i);
    }

    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        let mut prefix = Vec::new();
        self.use_tree(&mut prefix, &i.tree);
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if let Some((_, items)) = &i.content {
            self.module.push(i.ident.to_string());
            for it in items {
                self.visit_item(it);
            }
            self.module.pop();
        }
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        if segs.len() > 1 {
            self.record_path(&segs);
        }
        syn::visit::visit_path(self, p);
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        let fields = match &i.fields {
            syn::Fields::Named(f) => f.named.len(),
            syn::Fields::Unnamed(f) => f.unnamed.len(),
            syn::Fields::Unit => 0,
        };
        self.structs
            .push((i.ident.to_string(), i.span().start().line, fields));
        syn::visit::visit_item_struct(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_none() {
            if let syn::Type::Path(tp) = &*i.self_ty {
                if let Some(seg) = tp.path.segments.last() {
                    let n = i
                        .items
                        .iter()
                        .filter(|it| matches!(it, syn::ImplItem::Fn(f) if !is_cfg_test(&f.attrs)))
                        .count();
                    self.impls.push((seg.ident.to_string(), n));
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            text: text.into(),
        }
    }

    #[test]
    fn a_file_path_names_its_module() {
        assert_eq!(module_of("src/lib.rs"), Some(vec![]));
        assert_eq!(module_of("src/a/mod.rs"), Some(vec!["a".into()]));
        assert_eq!(module_of("src/a/b.rs"), Some(vec!["a".into(), "b".into()]));
        assert_eq!(module_of("src/bin/x.rs"), None);
    }

    #[test]
    fn code_lines_skip_blank_comment_and_test_items() {
        let src = "// c\n\nfn a() {}\n#[cfg(test)]\nmod t {\n    fn b() {}\n    fn c() {}\n}\nfn d() {}\n";
        let (m, _) = measure(&[file("src/lib.rs", src)]).unwrap();
        assert_eq!(m[0].code_lines, 2, "a and d only: {m:?}");
    }

    #[test]
    fn a_cfg_not_test_item_is_not_a_test_item() {
        let src = "#[cfg(not(test))]\nfn a() {}\n";
        let (m, _) = measure(&[file("src/lib.rs", src)]).unwrap();
        assert_eq!(m[0].code_lines, 2);
    }

    #[test]
    fn fan_out_and_fan_in_follow_use_and_paths() {
        let files = [
            file("src/lib.rs", "mod a;\nmod b;\n"),
            file("src/a.rs", "use crate::b::X;\nfn f() { crate::b::g(); }\n"),
            file("src/b.rs", "pub struct X;\npub fn g() {}\n"),
        ];
        let (m, _) = measure(&files).unwrap();
        assert_eq!(m[1].fan_out, BTreeSet::from(["b".to_string()]));
        assert_eq!(m[2].fan_in, 1, "{m:?}");
        assert_eq!(m[0].fan_in, 0);
    }

    #[test]
    fn a_child_naming_its_parent_is_containment_not_fan_out() {
        let files = [
            file("src/lib.rs", "mod a;\n"),
            file("src/a/mod.rs", "mod c;\npub struct P;\n"),
            file("src/a/c.rs", "use super::P;\nuse crate::a::P as Q;\n"),
        ];
        let (m, _) = measure(&files).unwrap();
        assert!(m[2].fan_out.is_empty(), "{:?}", m[2].fan_out);
    }

    #[test]
    fn super_in_an_inline_module_resolves_one_level_up() {
        let files = [
            file("src/lib.rs", "mod a;\nmod b;\n"),
            file("src/a.rs", "mod inner { use super::super::b::X; }\n"),
            file("src/b.rs", "pub struct X;\n"),
        ];
        let (m, _) = measure(&files).unwrap();
        assert_eq!(m[1].fan_out, BTreeSet::from(["b".to_string()]));
    }

    #[test]
    fn a_type_counts_fields_and_inherent_methods_but_not_trait_methods() {
        let src = "pub struct S { a: u8, b: u8 }\nimpl S { fn x(&self) {} fn y(&self) {} }\nimpl Clone for S { fn clone(&self) -> S { todo!() } }\n";
        let (m, _) = measure(&[file("src/lib.rs", src)]).unwrap();
        assert_eq!((m[0].types[0].fields, m[0].types[0].methods), (2, 2));
    }

    #[test]
    fn a_name_two_structs_share_is_noted_and_counts_no_methods() {
        let files = [
            file("src/lib.rs", "mod a;\nmod b;\n"),
            file("src/a.rs", "pub struct S;\nimpl S { fn x(&self) {} }\n"),
            file("src/b.rs", "pub struct S;\n"),
        ];
        let (m, notes) = measure(&files).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(m[1].types[0].methods, 0);
    }

    #[test]
    fn a_file_syn_cannot_parse_is_an_error_not_a_skip() {
        assert!(measure(&[file("src/lib.rs", "fn (")]).is_err());
    }
}
