//! Decides which dead-code candidates NOT to report because a target the lib build never compiled
//! (a bin, a test, an example, a bench) reaches them.
//!
//! Name-based reachability over the WHOLE library. Start from the identifiers the foreign files
//! mention; every item of the lib that carries one of those names makes the identifiers in its own
//! source reachable too, and so on to a fixpoint. A candidate is rescued when its name is reachable.
//!
//! It must run over every item, not only over the candidates: the path from a bin to a candidate
//! usually passes through items rustc did not report (a trait impl's method body, say).
//!
//! Edges beyond "an item's source mentions a name":
//! - `macro_rules! m { .. }` is an item named `m`: invoking `m!` reaches whatever its body names. A
//!   macro INVOCATION at item position is treated as always compiled, so its tokens are roots;
//! - `use a::b as c` makes `c` lead to `b`;
//! - an inherent `impl<T: Bound> S<T> { fn m }` gives `m` the identifiers of its header (`Bound`) but
//!   does NOT make `m` live by itself: a dead method of a live type must still be reported;
//! - an `impl Trait for Type` block is live when the lib type it implements is reachable, or, when
//!   the type is not defined in the lib (a primitive, a std type, a bare generic `T`), when the trait
//!   is. Then every identifier in the block is reachable (nobody mentions `drop`/`fmt` by name).
//!   Keywords and the impl's own generic parameters never count as the header's identifiers.
//!
//! Names, not paths, and heuristics at the edges (proc-macro and derive expansions are invisible):
//! it aims to over-rescue (miss a finding) rather than under-rescue (report a live item as dead),
//! and the module docs of `reachability` list what is known to escape that aim.

use super::diagnostics::DeadCodeHit;
use super::foreign::identifiers_in;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;

pub struct Split {
    pub kept: Vec<DeadCodeHit>,
    pub rescued: Vec<DeadCodeHit>,
}

/// `lib_files`: every file the lib compiles (`foreign::lib_module_files`).
pub fn rescue(
    hits: Vec<DeadCodeHit>,
    foreign: &BTreeSet<String>,
    root: &Path,
    lib_files: &BTreeSet<String>,
) -> anyhow::Result<Split> {
    let mut graph = Graph::default();
    for rel in lib_files {
        graph.add_file(root, rel)?;
    }
    let reachable = graph.reachable_from(foreign);

    let mut split = Split {
        kept: Vec::new(),
        rescued: Vec::new(),
    };
    for hit in hits {
        if reachable.contains(&hit.symbol) {
            split.rescued.push(hit);
        } else {
            split.kept.push(hit);
        }
    }
    Ok(split)
}

struct TraitImpl {
    /// Identifiers of the implemented type, minus the impl's own generic parameters.
    self_types: BTreeSet<String>,
    /// Identifiers of the implemented trait's path, minus the impl's own generic parameters.
    traits: BTreeSet<String>,
    /// Every identifier in the whole block.
    body: BTreeSet<String>,
}

#[derive(Default)]
struct Graph {
    /// name -> the identifiers inside each item (or macro, or `use` rename) carrying that name.
    items: BTreeMap<String, Vec<BTreeSet<String>>>,
    trait_impls: Vec<TraitImpl>,
    /// Names of the lib's own structs, enums, unions and type aliases.
    types: BTreeSet<String>,
    /// Identifiers inside item-position macro invocations: always compiled, so always reached.
    roots: BTreeSet<String>,
}

impl Graph {
    fn add_file(&mut self, root: &Path, rel: &str) -> anyhow::Result<()> {
        let source = String::from_utf8_lossy(&std::fs::read(root.join(rel))?).into_owned();
        let parsed = syn::parse_file(&source)
            .map_err(|e| anyhow::anyhow!("could not parse {rel} with syn ({e})"))?;
        let lines: Vec<&str> = source.lines().collect();
        let mut collector = Collector {
            lines: &lines,
            graph: self,
            inherent_headers: Vec::new(),
        };
        collector.visit_file(&parsed);
        Ok(())
    }

    fn impl_is_live(&self, imp: &TraitImpl, reached: &BTreeSet<String>) -> bool {
        let lib_types: Vec<&String> = imp
            .self_types
            .iter()
            .filter(|t| self.types.contains(*t))
            .collect();
        if lib_types.is_empty() {
            imp.traits.iter().any(|t| reached.contains(t))
        } else {
            lib_types.iter().any(|t| reached.contains(*t))
        }
    }

    fn reachable_from(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = seeds.union(&self.roots).cloned().collect();
        let mut done_impls = vec![false; self.trait_impls.len()];
        loop {
            while let Some(name) = queue.pop() {
                if !reached.insert(name.clone()) {
                    continue;
                }
                for idents in self.items.get(&name).into_iter().flatten() {
                    queue.extend(idents.iter().filter(|i| !reached.contains(*i)).cloned());
                }
            }
            let mut grew = false;
            for (i, imp) in self.trait_impls.iter().enumerate() {
                if !done_impls[i] && self.impl_is_live(imp, &reached) {
                    done_impls[i] = true;
                    queue.extend(imp.body.iter().filter(|b| !reached.contains(*b)).cloned());
                    grew = true;
                }
            }
            if !grew {
                return reached;
            }
        }
    }
}

struct Collector<'a> {
    lines: &'a [&'a str],
    graph: &'a mut Graph,
    /// Header identifiers of the inherent impls we are inside, innermost last.
    inherent_headers: Vec<BTreeSet<String>>,
}

impl Collector<'_> {
    /// Identifiers on lines `first..=last` (1-based, inclusive).
    fn idents(&self, first: usize, last: usize) -> BTreeSet<String> {
        let last = last.min(self.lines.len());
        if first == 0 || first > last {
            return BTreeSet::new();
        }
        identifiers_in(&self.lines[first - 1..last].join("\n"))
    }

    fn record(&mut self, ident: &syn::Ident, whole: proc_macro2::Span) {
        let mut idents = self.idents(whole.start().line, whole.end().line);
        if let Some(header) = self.inherent_headers.last() {
            idents.extend(header.iter().cloned());
        }
        self.graph
            .items
            .entry(clean(ident))
            .or_default()
            .push(idents);
    }

    fn record_type(&mut self, ident: &syn::Ident) {
        self.graph.types.insert(clean(ident));
    }
}

fn clean(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_string()
}

/// Every identifier in the path segments of a type or a trait path (`Vec<Guard>` -> Vec, Guard).
struct PathIdents(BTreeSet<String>);

impl<'ast> Visit<'ast> for PathIdents {
    fn visit_path_segment(&mut self, seg: &'ast syn::PathSegment) {
        self.0.insert(clean(&seg.ident));
        syn::visit::visit_path_segment(self, seg);
    }
}

fn generic_names(generics: &syn::Generics) -> BTreeSet<String> {
    generics
        .params
        .iter()
        .filter_map(|p| match p {
            syn::GenericParam::Type(t) => Some(clean(&t.ident)),
            syn::GenericParam::Const(c) => Some(clean(&c.ident)),
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}

/// `use a::b as c;` / `use a::{b as c, d::e as f};` -> [(c, b), (f, e)]. `self as c` renames the
/// enclosing path segment.
fn use_renames(tree: &syn::UseTree, last: Option<&str>, out: &mut Vec<(String, String)>) {
    match tree {
        syn::UseTree::Path(p) => use_renames(&p.tree, Some(&clean(&p.ident)), out),
        syn::UseTree::Group(g) => {
            for t in &g.items {
                use_renames(t, last, out);
            }
        }
        syn::UseTree::Rename(r) => {
            let original = clean(&r.ident);
            let original = if original == "self" {
                last.map(str::to_string).unwrap_or(original)
            } else {
                original
            };
            out.push((clean(&r.rename), original));
        }
        syn::UseTree::Name(_) | syn::UseTree::Glob(_) => {}
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_item_fn(self, i);
    }
    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.record(&i.ident, i.span());
        self.record_type(&i.ident);
        syn::visit::visit_item_struct(self, i);
    }
    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.record(&i.ident, i.span());
        self.record_type(&i.ident);
        syn::visit::visit_item_enum(self, i);
    }
    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.record(&i.ident, i.span());
        self.record_type(&i.ident);
        syn::visit::visit_item_union(self, i);
    }
    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_trait(self, i);
    }
    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.record(&i.ident, i.span());
        self.record_type(&i.ident);
        syn::visit::visit_item_type(self, i);
    }
    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_const(self, i);
    }
    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_static(self, i);
    }
    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_impl_item_fn(self, i);
    }
    fn visit_impl_item_const(&mut self, i: &'ast syn::ImplItemConst) {
        self.record(&i.ident, i.span());
        syn::visit::visit_impl_item_const(self, i);
    }
    fn visit_impl_item_type(&mut self, i: &'ast syn::ImplItemType) {
        self.record(&i.ident, i.span());
        syn::visit::visit_impl_item_type(self, i);
    }
    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        let whole = i.span();
        match &i.ident {
            // `macro_rules! name { ... }`: invoking `name!` reaches whatever the body names.
            Some(name) => self.record(name, whole),
            // `thread_local! { .. }`, `my_macro!(..)` at item position: always compiled.
            None => {
                let idents = self.idents(whole.start().line, whole.end().line);
                self.graph.roots.extend(idents);
            }
        }
        syn::visit::visit_item_macro(self, i);
    }

    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        let mut renames = Vec::new();
        use_renames(&i.tree, None, &mut renames);
        for (alias, original) in renames {
            self.graph
                .items
                .entry(alias)
                .or_default()
                .push([original].into_iter().collect());
        }
        syn::visit::visit_item_use(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let whole = i.span();
        let generics = generic_names(&i.generics);
        let strip = |mut set: BTreeSet<String>| {
            set.retain(|n| !generics.contains(n));
            set
        };
        match &i.trait_ {
            Some((trait_path, _)) => {
                let mut self_types = PathIdents(BTreeSet::new());
                self_types.visit_type(&i.self_ty);
                let mut traits = PathIdents(BTreeSet::new());
                traits.visit_path(trait_path);
                self.graph.trait_impls.push(TraitImpl {
                    self_types: strip(self_types.0),
                    traits: strip(traits.0),
                    body: self.idents(whole.start().line, whole.end().line),
                });
                syn::visit::visit_item_impl(self, i);
            }
            None => {
                // Only the header (generics, bounds, where clauses, the self type), not the body.
                let header_last = i.brace_token.span.open().start().line;
                let header = strip(self.idents(whole.start().line, header_last));
                self.inherent_headers.push(header);
                syn::visit::visit_item_impl(self, i);
                self.inherent_headers.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(file: &str, line: u32, symbol: &str) -> DeadCodeHit {
        DeadCodeHit {
            file: file.into(),
            line,
            symbol: symbol.into(),
        }
    }

    fn names(hits: &[DeadCodeHit]) -> Vec<&str> {
        hits.iter().map(|h| h.symbol.as_str()).collect()
    }

    fn foreign(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// A one-file lib; returns (dir, lib_files).
    fn lib(source: &str) -> (tempfile::TempDir, BTreeSet<String>) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), source).unwrap();
        (tmp, ["src/lib.rs".to_string()].into_iter().collect())
    }

    fn run(source: &str, hits: &[(u32, &str)], seeds: &[&str]) -> Split {
        let (tmp, files) = lib(source);
        let hits = hits.iter().map(|(l, n)| hit("src/lib.rs", *l, n)).collect();
        rescue(hits, &foreign(seeds), tmp.path(), &files).unwrap()
    }

    const CHAIN: &str = "\
pub(crate) fn rescued_root() -> i32 {
    helper_b() + 1
}
pub(crate) fn helper_b() -> i32 { deep_c() }
pub(crate) fn deep_c() -> i32 { 3 }
pub(crate) fn dead_a() -> i32 { dead_b() }
pub(crate) fn dead_b() -> i32 { 4 }
pub(crate) fn unrelated() -> i32 { 5 }
";

    const CHAIN_HITS: [(u32, &str); 6] = [
        (1, "rescued_root"),
        (4, "helper_b"),
        (5, "deep_c"),
        (6, "dead_a"),
        (7, "dead_b"),
        (8, "unrelated"),
    ];

    #[test]
    fn a_candidate_a_foreign_file_names_is_rescued_and_so_is_everything_it_reaches() {
        let split = run(CHAIN, &CHAIN_HITS, &["rescued_root"]);
        assert_eq!(
            names(&split.rescued),
            vec!["rescued_root", "helper_b", "deep_c"]
        );
        assert_eq!(names(&split.kept), vec!["dead_a", "dead_b", "unrelated"]);
    }

    #[test]
    fn with_nothing_foreign_nothing_is_rescued() {
        let split = run(CHAIN, &CHAIN_HITS, &[]);
        assert!(split.rescued.is_empty());
        assert_eq!(split.kept.len(), 6);
    }

    #[test]
    fn a_mention_outside_the_reached_items_own_lines_does_not_rescue() {
        let split = run(CHAIN, &CHAIN_HITS, &["helper_b"]);
        assert_eq!(names(&split.rescued), vec!["helper_b", "deep_c"]);
    }

    #[test]
    fn a_name_shared_with_something_reachable_is_rescued_the_accepted_over_rescue() {
        let split = run(CHAIN, &CHAIN_HITS, &["unrelated"]);
        assert_eq!(names(&split.rescued), vec!["unrelated"]);
    }

    #[test]
    fn the_path_may_pass_through_an_item_that_is_not_a_candidate() {
        // bin -> entry (candidate) -> middle (live as far as rustc says, so NOT a candidate) -> leaf.
        let src = "\
pub(crate) fn entry() { middle(); }
fn middle() { leaf(); }
pub(crate) fn leaf() {}
pub(crate) fn orphan() {}
";
        let split = run(src, &[(1, "entry"), (3, "leaf"), (4, "orphan")], &["entry"]);
        assert_eq!(names(&split.rescued), vec!["entry", "leaf"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    const GUARD: &str = "\
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        cleanup();
    }
}
pub(crate) fn cleanup() {}
pub(crate) fn orphan() {}
";

    #[test]
    fn a_trait_impl_is_live_with_its_type_so_what_its_methods_call_is_reachable() {
        let split = run(
            GUARD,
            &[(1, "Guard"), (7, "cleanup"), (8, "orphan")],
            &["Guard"],
        );
        assert_eq!(names(&split.rescued), vec!["Guard", "cleanup"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    #[test]
    fn an_unreached_trait_impl_does_not_rescue_what_it_calls() {
        let split = run(GUARD, &[(1, "Guard"), (7, "cleanup")], &[]);
        assert_eq!(split.kept.len(), 2);
    }

    #[test]
    fn keywords_and_the_impls_own_generics_do_not_make_an_impl_live() {
        // Review finding: `impl`, `for` and a generic `T` appear in every header, and anything
        // reachable that mentions them made EVERY trait impl live. Those words must not count.
        let src = "\
pub(crate) struct Never<T>(T);
impl<T> Drop for Never<T> {
    fn drop(&mut self) {
        only_from_drop();
    }
}
pub(crate) fn only_from_drop() {}
";
        let hits = [(1, "Never"), (7, "only_from_drop")];
        let split = run(src, &hits, &["for", "impl", "T", "self", "Self"]);
        assert_eq!(split.kept.len(), 2, "{:?}", names(&split.rescued));
    }

    #[test]
    fn a_generic_parameter_of_the_impl_is_not_the_trait_reaching_anything() {
        // `impl<T> Helper<T> for u16`: `T` is the impl's own parameter. Something reachable that
        // mentions `T` (nearly everything generic does) must not make this impl live.
        let src = "pub(crate) trait Helper<X> { fn h(&self, x: X); }
impl<T> Helper<T> for u16 {
    fn h(&self, _x: T) { only_here(); }
}
pub(crate) fn only_here() {}
";
        let hits = [(1, "Helper"), (5, "only_here")];
        assert_eq!(run(src, &hits, &["T"]).kept.len(), 2);
        assert_eq!(
            names(&run(src, &hits, &["Helper"]).rescued),
            vec!["Helper", "only_here"]
        );
    }

    #[test]
    fn an_impl_for_a_type_outside_the_lib_is_live_when_its_trait_is_reachable() {
        let src = "\
pub(crate) trait Helper { fn h(&self) -> u8; }
impl Helper for u16 {
    fn h(&self) -> u8 { inner() }
}
pub(crate) fn inner() -> u8 { 1 }
";
        let hits = [(1, "Helper"), (5, "inner")];
        assert_eq!(
            names(&run(src, &hits, &["Helper"]).rescued),
            vec!["Helper", "inner"]
        );
        assert_eq!(run(src, &hits, &[]).kept.len(), 2);
    }

    #[test]
    fn a_dead_inherent_method_of_a_reached_type_is_still_reported() {
        let src = "\
pub(crate) struct S;
impl S {
    pub(crate) fn used() {}
    pub(crate) fn never_called() {}
}
";
        let split = run(
            src,
            &[(1, "S"), (3, "used"), (4, "never_called")],
            &["S", "used"],
        );
        assert_eq!(names(&split.rescued), vec!["S", "used"]);
        assert_eq!(names(&split.kept), vec!["never_called"]);
    }

    #[test]
    fn a_bound_in_an_inherent_impls_header_belongs_to_each_of_its_methods() {
        // Review finding: `Helper` is named only in the header of `impl<T: Helper> S<T>`.
        let src = "\
pub(crate) trait Helper { fn h(&self) -> u8; }
pub(crate) struct S<T>(T);
impl<T: Helper> S<T> {
    pub(crate) fn get(&self) -> u8 { 1 }
    pub(crate) fn other(&self) -> u8 { 2 }
}
";
        let hits = [(1, "Helper"), (4, "get"), (5, "other")];
        let split = run(src, &hits, &["get"]);
        assert_eq!(names(&split.rescued), vec!["Helper", "get"]);
        assert_eq!(names(&split.kept), vec!["other"]);
    }

    #[test]
    fn a_macro_rules_body_is_reached_when_the_macro_is_named() {
        // Review finding: `macro_rules!` bodies were invisible, so what a macro calls was reported.
        let src = "\
macro_rules! call_helper { () => { helper() } }
pub(crate) fn helper() {}
pub(crate) fn run() { call_helper!(); }
pub(crate) fn orphan() {}
";
        let hits = [(2, "helper"), (3, "run"), (4, "orphan")];
        let split = run(src, &hits, &["run"]);
        assert_eq!(names(&split.rescued), vec!["helper", "run"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    #[test]
    fn a_macro_invocation_at_item_position_is_always_compiled_so_its_tokens_are_roots() {
        let src = "\
make_things!(from_macro);
pub(crate) fn from_macro() {}
pub(crate) fn orphan() {}
";
        let split = run(src, &[(2, "from_macro"), (3, "orphan")], &[]);
        assert_eq!(names(&split.rescued), vec!["from_macro"]);
    }

    #[test]
    fn a_use_rename_leads_back_to_the_original_name() {
        let src = "\
mod inner {
    pub(crate) fn real() {}
    pub(crate) fn orphan() {}
}
pub(crate) use inner::real as nice;
use inner::{self as inn, orphan as o};
";
        let hits = [(2, "real"), (3, "orphan")];
        // reaching `nice` reaches `real` and nothing else
        assert_eq!(names(&run(src, &hits, &["nice"]).rescued), vec!["real"]);
        // reaching `o` (`orphan as o`) reaches `orphan`
        assert_eq!(names(&run(src, &hits, &["o"]).rescued), vec!["orphan"]);
        // `self as inn` renames the enclosing module; a module is not an item here, so nothing is rescued
        assert!(run(src, &hits, &["inn"]).rescued.is_empty());
    }

    #[test]
    fn a_candidate_with_no_item_of_its_own_is_rescued_only_by_a_reachable_name() {
        // dead_code also reports struct fields and enum variants; they are not items here.
        let src = "pub(crate) struct S {\n    field: i32,\n}\n";
        let hits = [(1, "S"), (2, "field")];
        assert_eq!(run(src, &hits, &[]).kept.len(), 2);
        // S's own source mentions `field`, so reaching S reaches the field
        assert_eq!(names(&run(src, &hits, &["S"]).rescued), vec!["S", "field"]);
    }

    #[test]
    fn a_multiline_items_whole_body_counts() {
        let src = "pub(crate) fn big() {\n    let a = 1;\n    let b = 2;\n    deep();\n}\npub(crate) fn deep() {}\n";
        let split = run(src, &[(1, "big"), (6, "deep")], &["big"]);
        assert_eq!(names(&split.rescued), vec!["big", "deep"]);
    }
}
