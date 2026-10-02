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
//! Two rules keep it sound in the direction that matters (never report a live item as dead):
//! - an `impl Trait for Type` block is live as soon as its header (the trait or the type) is
//!   reachable, so every identifier in the block becomes reachable. rustc never reports a trait
//!   impl's methods, and nothing calls `drop`/`fmt`/`default` by name;
//! - inherent `impl Type { fn m }` is NOT treated that way: `m` itself must be mentioned, otherwise a
//!   dead method of a live type would never be reported.
//!
//! Names, not paths: it can over-rescue (a dead `new` hidden because something reachable mentions
//! `new`), never under-rescue.

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

#[derive(Default)]
struct Graph {
    /// name -> the identifiers inside each item carrying that name.
    items: BTreeMap<String, Vec<BTreeSet<String>>>,
    /// (identifiers in the header, identifiers in the whole block) of every trait impl.
    trait_impls: Vec<(BTreeSet<String>, BTreeSet<String>)>,
}

impl Graph {
    fn add_file(&mut self, root: &Path, rel: &str) -> anyhow::Result<()> {
        let source = std::fs::read_to_string(root.join(rel))?;
        let parsed = syn::parse_file(&source)
            .map_err(|e| anyhow::anyhow!("could not parse {rel} with syn ({e})"))?;
        let lines: Vec<&str> = source.lines().collect();
        let mut collector = Collector {
            lines: &lines,
            graph: self,
        };
        collector.visit_file(&parsed);
        Ok(())
    }

    fn reachable_from(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut reached: BTreeSet<String> = BTreeSet::new();
        let mut queue: Vec<String> = seeds.iter().cloned().collect();
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
            for (i, (header, body)) in self.trait_impls.iter().enumerate() {
                if !done_impls[i] && header.iter().any(|h| reached.contains(h)) {
                    done_impls[i] = true;
                    queue.extend(body.iter().filter(|b| !reached.contains(*b)).cloned());
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
        let idents = self.idents(whole.start().line, whole.end().line);
        self.graph
            .items
            .entry(ident.to_string().trim_start_matches("r#").to_string())
            .or_default()
            .push(idents);
    }
}

impl<'ast> Visit<'ast> for Collector<'_> {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        self.record(&i.sig.ident, i.span());
        syn::visit::visit_item_fn(self, i);
    }
    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_struct(self, i);
    }
    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_enum(self, i);
    }
    fn visit_item_union(&mut self, i: &'ast syn::ItemUnion) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_union(self, i);
    }
    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        self.record(&i.ident, i.span());
        syn::visit::visit_item_trait(self, i);
    }
    fn visit_item_type(&mut self, i: &'ast syn::ItemType) {
        self.record(&i.ident, i.span());
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
    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if i.trait_.is_some() {
            let whole = i.span();
            let header_last = i.brace_token.span.open().start().line;
            let header = self.idents(whole.start().line, header_last);
            let body = self.idents(whole.start().line, whole.end().line);
            self.graph.trait_impls.push((header, body));
        }
        syn::visit::visit_item_impl(self, i);
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
        // The case a self-scan of CodeRipper exposed: bin -> entry (candidate) -> middle (live as far
        // as rustc says, so NOT a candidate) -> leaf (candidate). Only entry is named by the bin.
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

    #[test]
    fn a_trait_impl_is_live_with_its_type_so_what_its_methods_call_is_reachable() {
        // Nothing mentions `drop` by name; it runs because `Guard` is used. `cleanup` must be rescued.
        let src = "\
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        cleanup();
    }
}
pub(crate) fn cleanup() {}
pub(crate) fn orphan() {}
";
        let split = run(
            src,
            &[(1, "Guard"), (7, "cleanup"), (8, "orphan")],
            &["Guard"],
        );
        assert_eq!(names(&split.rescued), vec!["Guard", "cleanup"]);
        assert_eq!(names(&split.kept), vec!["orphan"]);
    }

    #[test]
    fn an_unreached_trait_impl_does_not_rescue_what_it_calls() {
        let src = "\
pub(crate) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        cleanup();
    }
}
pub(crate) fn cleanup() {}
";
        let split = run(src, &[(1, "Guard"), (7, "cleanup")], &[]);
        assert_eq!(split.kept.len(), 2);
    }

    #[test]
    fn a_dead_inherent_method_of_a_reached_type_is_still_reported() {
        // The type is used by the bin; its method `never_called` is mentioned by nobody.
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
