//! Tags every eligible function with `#[must_use = "CR:<id>"] #[deprecated(note = "CR:<id>")]`,
//! inserted INLINE (never as a new line) so every original line number survives the rewrite.
//!
//! Why both attributes: `unused_must_use` fires only where a call's value is discarded; `deprecated`
//! fires at every use of the function. Ignored-sites / all-uses is what separates "never consumed"
//! from "consumed somewhere" -- `unused_must_use` alone can't (used sites are silent).

use proc_macro2::LineColumn;
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// One function that was tagged. `id` is the number in its `CR:<id>` reason string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnTag {
    pub id: u32,
    /// Forward-slash path relative to the project root, e.g. `src/lib.rs`.
    pub file: String,
    pub name: String,
    /// 1-based line of the function's name.
    pub line: u32,
    /// 1-based last line of the function's body (== `line` for a bodiless trait method).
    pub last_line: u32,
}

/// A `use` item's line span (inclusive). rustc reports `deprecated` at every imported name, which
/// is not a call site -- the classifier drops hits inside these ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseRange {
    pub file: String,
    pub first_line: u32,
    pub last_line: u32,
}

#[derive(Debug)]
pub struct Annotated {
    pub source: String,
    pub tags: Vec<FnTag>,
    pub use_ranges: Vec<UseRange>,
}

/// Annotates one file. `next_id` is shared across files so ids are unique per run.
pub fn annotate(file: &str, source: &str, next_id: &mut u32) -> anyhow::Result<Annotated> {
    let parsed = syn::parse_file(source)
        .map_err(|e| anyhow::anyhow!("could not parse {file} with syn ({e}); refusing to guess"))?;

    let mut visitor = Annotator {
        file,
        next_id,
        insertions: Vec::new(),
        tags: Vec::new(),
        use_ranges: Vec::new(),
    };
    visitor.visit_file(&parsed);

    let Annotator {
        mut insertions,
        tags,
        use_ranges,
        ..
    } = visitor;
    Ok(Annotated {
        source: apply_insertions(source, &mut insertions),
        tags,
        use_ranges,
    })
}

/// `use` item line ranges of a file that is NOT rewritten (`tests/`, `examples/`, `benches/`):
/// an import there is no more a call site than one in `src/`.
pub fn use_ranges_only(file: &str, source: &str) -> anyhow::Result<Vec<UseRange>> {
    let parsed = syn::parse_file(source)
        .map_err(|e| anyhow::anyhow!("could not parse {file} with syn ({e}); refusing to guess"))?;
    let mut visitor = Annotator {
        file,
        next_id: &mut 0,
        insertions: Vec::new(),
        tags: Vec::new(),
        use_ranges: Vec::new(),
    };
    visitor.visit_file(&parsed);
    Ok(visitor.use_ranges)
}

struct Insertion {
    at: LineColumn,
    text: String,
}

struct Annotator<'a> {
    file: &'a str,
    next_id: &'a mut u32,
    insertions: Vec<Insertion>,
    tags: Vec<FnTag>,
    use_ranges: Vec<UseRange>,
}

/// Attribute names (last path segment) that mean "leave this function alone".
const SKIP_ATTRS: &[&str] = &[
    "must_use",
    "deprecated",
    "test",
    "bench",
    "main",
    "no_mangle",
    "export_name",
    "proc_macro",
    "proc_macro_derive",
    "proc_macro_attribute",
];

impl Annotator<'_> {
    fn consider(
        &mut self,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        sig: &syn::Signature,
        last_line: u32,
    ) {
        if !is_eligible(attrs, sig) {
            return;
        }
        // Attributes must come before the visibility, so insert before `pub` when there is one.
        let at = if matches!(vis, syn::Visibility::Inherited) {
            sig.span().start()
        } else {
            vis.span().start()
        };
        let id = *self.next_id;
        *self.next_id += 1;
        self.insertions.push(Insertion {
            at,
            text: format!("#[must_use = \"CR:{id}\"] #[deprecated(note = \"CR:{id}\")] "),
        });
        self.tags.push(FnTag {
            id,
            file: self.file.to_string(),
            name: sig.ident.to_string(),
            line: sig.ident.span().start().line as u32,
            last_line,
        });
    }
}

impl<'ast> Visit<'ast> for Annotator<'_> {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let last = f.block.brace_token.span.close().end().line as u32;
        self.consider(&f.attrs, &f.vis, &f.sig, last);
        syn::visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if f.modifiers.defaultness.is_none() {
            let last = f.block.brace_token.span.close().end().line as u32;
            self.consider(&f.attrs, &f.vis, &f.sig, last);
        }
        syn::visit::visit_impl_item_fn(self, f);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        let last = match &f.default {
            Some(block) => block.brace_token.span.close().end().line as u32,
            None => f.sig.ident.span().start().line as u32,
        };
        self.consider(&f.attrs, &syn::Visibility::Inherited, &f.sig, last);
        syn::visit::visit_trait_item_fn(self, f);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        // A trait impl's methods are reached through the trait declaration, which is tagged.
        // Tagging them too would double-count every call.
        if i.trait_.is_none() {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        self.use_ranges.push(UseRange {
            file: self.file.to_string(),
            first_line: u.use_token.span.start().line as u32,
            last_line: u.semi_token.span.end().line as u32,
        });
    }
}

fn is_eligible(attrs: &[syn::Attribute], sig: &syn::Signature) -> bool {
    if sig.asyncness.is_some() || sig.abi.is_some() || sig.ident == "main" {
        return false;
    }
    let skipped_attrs: HashSet<&str> = SKIP_ATTRS.iter().copied().collect();
    let has_skip_attr = attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|seg| skipped_attrs.contains(seg.ident.to_string().as_str()))
    });
    if has_skip_attr {
        return false;
    }
    returns_a_value_worth_tracking(&sig.output)
}

/// No `->`, `-> ()`, `-> !`, a `Result` (rustc already warns on ignoring it by default) and
/// `-> &mut Self` (builder chaining, idiomatically ignored) are not worth tracking.
fn returns_a_value_worth_tracking(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    match ty.as_ref() {
        syn::Type::Never(_) => false,
        syn::Type::Tuple(t) if t.elems.is_empty() => false,
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .is_none_or(|seg| seg.ident != "Result"),
        syn::Type::Reference(r) if r.mutability.is_some() => !matches!(
            r.elem.as_ref(),
            syn::Type::Path(p) if p.path.is_ident("Self")
        ),
        _ => true,
    }
}

/// Applies insertions from the last position to the first so earlier columns stay valid.
/// `LineColumn.column` counts characters, not bytes, so convert per line.
fn apply_insertions(source: &str, insertions: &mut [Insertion]) -> String {
    insertions.sort_by_key(|i| std::cmp::Reverse((i.at.line, i.at.column)));
    let mut lines: Vec<String> = source.split_inclusive('\n').map(str::to_string).collect();
    for ins in insertions.iter() {
        let line = &mut lines[ins.at.line - 1];
        let byte = line
            .char_indices()
            .nth(ins.at.column)
            .map_or(line.len(), |(b, _)| b);
        line.insert_str(byte, &ins.text);
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Annotated {
        annotate("src/lib.rs", src, &mut 0).unwrap()
    }

    const TAG0: &str = "#[must_use = \"CR:0\"] #[deprecated(note = \"CR:0\")] ";

    #[test]
    fn a_plain_function_is_tagged_inline_and_keeps_its_line_number() {
        let out = run("pub fn a() -> i32 { 1 }\n");
        assert_eq!(out.source, format!("{TAG0}pub fn a() -> i32 {{ 1 }}\n"));
        assert_eq!(out.tags.len(), 1);
        assert_eq!((out.tags[0].name.as_str(), out.tags[0].line), ("a", 1));
    }

    #[test]
    fn the_tag_goes_after_existing_attributes_and_before_pub_crate() {
        let out = run("/// doc\n#[inline]\n    pub(crate) fn a() -> i32 { 1 }\n");
        assert_eq!(
            out.source,
            format!("/// doc\n#[inline]\n    {TAG0}pub(crate) fn a() -> i32 {{ 1 }}\n")
        );
        assert_eq!(out.tags[0].line, 3);
    }

    #[test]
    fn inherent_methods_and_trait_declarations_are_tagged_but_trait_impl_methods_are_not() {
        let src = "struct S;\nimpl S {\n    fn m(&self) -> i32 { 1 }\n}\ntrait T {\n    fn t(&self) -> i32;\n}\nimpl T for S {\n    fn t(&self) -> i32 { 2 }\n}\n";
        let out = run(src);
        let names: Vec<_> = out.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["m", "t"]);
        assert_eq!(out.tags[1].line, 6, "the trait DECLARATION, not the impl");
        assert_eq!(out.source.matches("#[deprecated").count(), 2);
    }

    #[test]
    fn nested_functions_and_functions_in_modules_are_tagged() {
        let out = run("mod m {\n    pub fn a() -> i32 {\n        fn inner() -> i32 { 1 }\n        inner()\n    }\n}\n");
        let names: Vec<_> = out.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["a", "inner"]);
        assert_eq!(out.tags[0].last_line, 5, "body spans lines 2..=5");
    }

    #[test]
    fn ids_continue_across_files() {
        let mut next = 0;
        let a = annotate("src/a.rs", "fn a() -> i32 { 1 }\n", &mut next).unwrap();
        let b = annotate("src/b.rs", "fn b() -> i32 { 1 }\n", &mut next).unwrap();
        assert_eq!((a.tags[0].id, b.tags[0].id, next), (0, 1, 2));
    }

    #[test]
    fn two_functions_on_one_line_are_both_tagged() {
        let out = run("fn a() -> i32 { 1 } fn b() -> i32 { 2 }\n");
        assert_eq!(out.tags.len(), 2);
        assert_eq!(out.source.matches("#[must_use").count(), 2);
    }

    #[test]
    fn columns_are_characters_not_bytes_and_crlf_is_preserved() {
        let out = run("/* é */ fn a() -> i32 { 1 }\r\nfn b() -> i32 { 2 }\r\n");
        assert!(out.source.starts_with(&format!("/* é */ {TAG0}fn a()")));
        assert!(out.source.contains("{ 1 }\r\n"));
        assert!(out.source.ends_with("{ 2 }\r\n"));
    }

    #[test]
    fn functions_not_worth_tracking_are_left_untouched() {
        let cases = [
            "fn f() {}\n",
            "fn f() -> () {}\n",
            "fn f() -> ! { loop {} }\n",
            "async fn f() -> i32 { 1 }\n",
            "fn f() -> Result<i32, ()> { Ok(1) }\n",
            "fn f() -> std::io::Result<i32> { Ok(1) }\n",
            "struct B;\nimpl B { fn f(&mut self) -> &mut Self { self } }\n",
            "fn main() -> i32 { 1 }\n",
            "#[test]\nfn f() -> i32 { 1 }\n",
            "#[tokio::main]\nfn f() -> i32 { 1 }\n",
            "#[must_use]\nfn f() -> i32 { 1 }\n",
            "#[deprecated]\nfn f() -> i32 { 1 }\n",
            "extern \"C\" fn f() -> i32 { 1 }\n",
            "#[no_mangle]\npub fn f() -> i32 { 1 }\n",
        ];
        for src in cases {
            let out = run(src);
            assert_eq!(out.source, src, "should be untouched: {src:?}");
            assert!(out.tags.is_empty(), "no tag expected: {src:?}");
        }
    }

    #[test]
    fn a_function_returning_a_mutable_reference_to_something_else_is_still_tracked() {
        let out = run("fn f(v: &mut Vec<i32>) -> &mut i32 { &mut v[0] }\n");
        assert_eq!(out.tags.len(), 1);
    }

    #[test]
    fn use_statements_are_recorded_including_multiline_ones() {
        let out = run("use a::b;\npub use c::{\n    d,\n};\nfn f() -> i32 { 1 }\n");
        assert_eq!(
            out.use_ranges,
            vec![
                UseRange {
                    file: "src/lib.rs".into(),
                    first_line: 1,
                    last_line: 1
                },
                UseRange {
                    file: "src/lib.rs".into(),
                    first_line: 2,
                    last_line: 4
                },
            ]
        );
    }

    #[test]
    fn use_ranges_only_records_imports_without_rewriting_anything() {
        let ranges = use_ranges_only(
            "tests/t.rs",
            "use a::b;
fn f() -> i32 { 1 }
",
        )
        .unwrap();
        assert_eq!(
            ranges,
            vec![UseRange {
                file: "tests/t.rs".into(),
                first_line: 1,
                last_line: 1
            }]
        );
    }

    #[test]
    fn a_file_syn_cannot_parse_is_an_error_not_a_silent_skip() {
        assert!(annotate("src/lib.rs", "this is not rust {{{", &mut 0).is_err());
    }
}
