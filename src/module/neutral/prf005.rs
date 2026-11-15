// SPDX-License-Identifier: MIT OR Apache-2.0
//! PRF-005: a cache or a process-lifetime collection has a way to shrink.
//!
//! **What this judges (Rust, from the source alone):** every tracked production `.rs` file (outside vendored directories, tests,
//! examples, tools, `build.rs`, files over 2 MiB, and outside `#[cfg(test)]` and `#[test]` items) is parsed with `syn`. A
//! **candidate** is a growable collection (`Vec`, `VecDeque`, `HashMap`, `HashSet`, `BTreeMap`, `BTreeSet`, `BinaryHeap`,
//! `LinkedList`, `IndexMap`, `IndexSet`, `DashMap`, `DashSet`, looked through `Mutex`, `RwLock`, `RefCell`, `Cell`, `Arc`, `Rc`,
//! `LazyLock`, `LazyCell`, `OnceLock`, `OnceCell` and `Lazy`) that is one of:
//!
//! 1. a `static` item or a `thread_local!` static, whatever its name (it lives as long as the process);
//! 2. a named field whose own name contains `cache` or `memo`, or any field of a struct whose name contains `cache` or `memo`.
//!
//! A candidate is **grown** where its name is the receiver of `insert`, `push`, `push_back`, `push_front`, `extend`,
//! `extend_from_slice`, `append` or `entry` (through `.lock()`, `.borrow_mut()`, `.unwrap()`, a `let` binding of such a receiver, or
//! the closure of `NAME.with(|c| ..)`), and **bounded** where the same name is the receiver of `remove`, `remove_entry`, `pop`,
//! `pop_front`, `pop_back`, `pop_first`, `pop_last`, `clear`, `retain`, `retain_mut`, `truncate`, `drain`, `split_off`,
//! `swap_remove`, `capacity`, `is_full`, or a method starting `evict`, `expire`, `purge`, `prune`, `trim`, `cleanup`, `reap` or
//! `gc`; is compared by `len()` with `<`, `<=`, `>`, `>=`, `==` or `!=`; is assigned a new value; or is passed to `mem::take`,
//! `mem::replace` or `mem::swap`. **One Medium finding at low confidence per candidate that is grown and never bounded**, at its
//! declaration; the subject is `Owner::field` for a field and the static's name otherwise.
//!
//! **What it cannot see:** names are matched across the whole repository by identifier, so a bound on another type's field of the
//! same name hides a candidate (a miss, never a false report); a growth or a bound through a function that returns the collection,
//! through a macro, or through an alias the rule does not follow; whether the collection takes external input (the catalog's
//! claim is about collections that do, which needs a taint analysis this rule does not do); a bound enforced by the type
//! (`lru::LruCache`, `moka`, a bounded channel are not collections here and are never reported); a collection in a local variable
//! or in a field that is not named like a cache. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_src::{is_test_item, line_of, production_files, MAX_BYTES};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use syn::visit::Visit;

const ID: &str = crate::rule_id::PRF_005;

const COLLECTIONS: &[&str] = &[
    "Vec",
    "VecDeque",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "LinkedList",
    "IndexMap",
    "IndexSet",
    "DashMap",
    "DashSet",
];

const WRAPPERS: &[&str] = &[
    "Mutex", "RwLock", "RefCell", "Cell", "Arc", "Rc", "LazyLock", "LazyCell", "OnceLock",
    "OnceCell", "Lazy",
];

const GROWS: &[&str] = &[
    "insert",
    "push",
    "push_back",
    "push_front",
    "extend",
    "extend_from_slice",
    "append",
    "entry",
];

const SHRINKS: &[&str] = &[
    "remove",
    "remove_entry",
    "pop",
    "pop_front",
    "pop_back",
    "pop_first",
    "pop_last",
    "clear",
    "retain",
    "retain_mut",
    "truncate",
    "drain",
    "split_off",
    "swap_remove",
    "capacity",
    "is_full",
];

const SHRINK_PREFIXES: &[&str] = &[
    "evict", "expire", "purge", "prune", "trim", "cleanup", "reap", "gc",
];

/// The closure-taking accessors of a `thread_local!` static and its relatives.
const WITH: &[&str] = &["with", "with_borrow", "with_borrow_mut"];

/// The collection a type is, looking through the wrappers a shared collection is kept in.
fn collection_of(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Paren(p) => collection_of(&p.elem),
        syn::Type::Group(g) => collection_of(&g.elem),
        syn::Type::Path(p) if p.qself.is_none() => {
            let last = p.path.segments.last()?;
            let name = last.ident.to_string();
            if COLLECTIONS.contains(&name.as_str()) {
                return Some(name);
            }
            if !WRAPPERS.contains(&name.as_str()) {
                return None;
            }
            match &last.arguments {
                syn::PathArguments::AngleBracketed(a) => a.args.iter().find_map(|g| match g {
                    syn::GenericArgument::Type(t) => collection_of(t),
                    _ => None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The identifiers an expression reaches through fields, paths, method receivers, references, dereferences and indexing, outermost
/// first: `self.cache.lock().unwrap()` gives `cache`, `self`.
fn chain(e: &syn::Expr, out: &mut Vec<String>) {
    match e {
        syn::Expr::Field(f) => {
            if let syn::Member::Named(i) = &f.member {
                out.push(i.to_string());
            }
            chain(&f.base, out);
        }
        syn::Expr::MethodCall(m) => chain(&m.receiver, out),
        syn::Expr::Path(p) if p.qself.is_none() => {
            if let Some(s) = p.path.segments.last() {
                out.push(s.ident.to_string());
            }
        }
        syn::Expr::Paren(p) => chain(&p.expr, out),
        syn::Expr::Reference(r) => chain(&r.expr, out),
        syn::Expr::Unary(u) => chain(&u.expr, out),
        syn::Expr::Try(t) => chain(&t.expr, out),
        syn::Expr::Index(i) => chain(&i.expr, out),
        syn::Expr::Await(a) => chain(&a.base, out),
        _ => {}
    }
}

fn pattern_names(p: &syn::Pat, out: &mut Vec<String>) {
    match p {
        syn::Pat::Ident(i) => out.push(i.ident.to_string()),
        syn::Pat::TupleStruct(t) => t.elems.iter().for_each(|e| pattern_names(e, out)),
        syn::Pat::Tuple(t) => t.elems.iter().for_each(|e| pattern_names(e, out)),
        syn::Pat::Reference(r) => pattern_names(&r.pat, out),
        syn::Pat::Type(t) => pattern_names(&t.pat, out),
        _ => {}
    }
}

/// A name that says it is a cache or a memo table: `cache`, `memo`, `memoize`, but not `memory` (`MemoryStore`, `InMemoryLimiter`).
fn cachey(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("cache") || lower.replace("memory", "").contains("memo")
}

fn shrinking(method: &str) -> bool {
    SHRINKS.contains(&method) || SHRINK_PREFIXES.iter().any(|p| method.starts_with(p))
}

struct Decl {
    file: String,
    line: u32,
    /// The struct a field belongs to, none for a static.
    owner: Option<String>,
    name: String,
    kind: String,
}

#[derive(Default)]
struct Walk {
    file: String,
    decls: Vec<Decl>,
    /// Per name, the lines it is grown at.
    grown: HashMap<String, Vec<u32>>,
    bounded: BTreeSet<String>,
    /// The bindings of the function being read, to the names their initialiser reached.
    aliases: HashMap<String, Vec<String>>,
    /// The closure parameters of the `NAME.with(|c| ..)` calls being read, to `NAME`.
    frames: Vec<(String, Vec<String>)>,
}

impl Walk {
    /// The names an expression stands for: what it reaches, plus what a binding or a `with` closure parameter at its root stands for.
    fn names(&self, e: &syn::Expr) -> Vec<String> {
        let mut out = Vec::new();
        chain(e, &mut out);
        let Some(root) = out.last().cloned() else {
            return out;
        };
        if let Some(a) = self.aliases.get(&root) {
            out.extend(a.iter().cloned());
        }
        for (param, names) in self.frames.iter().rev() {
            if *param == root {
                out.extend(names.iter().cloned());
            }
        }
        out
    }

    fn bound(&mut self, e: &syn::Expr) {
        for n in self.names(e) {
            self.bounded.insert(n);
        }
    }

    fn declare(
        &mut self,
        span: proc_macro2::Span,
        owner: Option<String>,
        name: String,
        ty: &syn::Type,
    ) {
        if let Some(kind) = collection_of(ty) {
            self.decls.push(Decl {
                file: self.file.clone(),
                line: line_of(span),
                owner,
                name,
                kind,
            });
        }
    }

    fn bindings(&mut self, pat: &syn::Pat, init: &syn::Expr) {
        let names = self.names(init);
        if names.is_empty() {
            return;
        }
        let mut idents = Vec::new();
        pattern_names(pat, &mut idents);
        for i in idents {
            self.aliases.insert(i, names.clone());
        }
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_test_item(&i.attrs) {
            return;
        }
        let saved = std::mem::take(&mut self.aliases);
        syn::visit::visit_item_fn(self, i);
        self.aliases = saved;
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_test_item(&i.attrs) {
            return;
        }
        let saved = std::mem::take(&mut self.aliases);
        syn::visit::visit_impl_item_fn(self, i);
        self.aliases = saved;
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_trait_item_fn(self, i);
        }
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        if is_test_item(&i.attrs) {
            return;
        }
        let span = i.ident.span();
        self.declare(span, None, i.ident.to_string(), &i.ty);
        syn::visit::visit_item_static(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        if !i.mac.path.is_ident("thread_local") {
            return;
        }
        let parsed = syn::parse::Parser::parse2(
            |input: syn::parse::ParseStream| {
                let mut statics = Vec::new();
                while !input.is_empty() {
                    statics.push(input.parse::<syn::ItemStatic>()?);
                }
                Ok(statics)
            },
            i.mac.tokens.clone(),
        );
        if let Ok(statics) = parsed {
            for s in &statics {
                self.declare(s.ident.span(), None, s.ident.to_string(), &s.ty);
            }
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if is_test_item(&i.attrs) {
            return;
        }
        let owner = i.ident.to_string();
        let owner_cachey = cachey(&owner);
        if let syn::Fields::Named(fields) = &i.fields {
            for f in &fields.named {
                let Some(name) = f.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                if owner_cachey || cachey(&name) {
                    self.declare(
                        syn::spanned::Spanned::span(f),
                        Some(owner.clone()),
                        name,
                        &f.ty,
                    );
                }
            }
        }
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let Some(init) = &l.init {
            self.bindings(&l.pat, &init.expr);
        }
        syn::visit::visit_local(self, l);
    }

    fn visit_expr_let(&mut self, e: &'ast syn::ExprLet) {
        self.bindings(&e.pat, &e.expr);
        syn::visit::visit_expr_let(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        let method = e.method.to_string();
        let names = self.names(&e.receiver);
        if GROWS.contains(&method.as_str()) {
            let line = line_of(e.method.span());
            for n in &names {
                self.grown.entry(n.clone()).or_default().push(line);
            }
        }
        if shrinking(&method) {
            self.bounded.extend(names.iter().cloned());
        }
        let param = if WITH.contains(&method.as_str()) {
            e.args.iter().find_map(|a| match a {
                syn::Expr::Closure(c) => c.inputs.first().and_then(|p| {
                    let mut ids = Vec::new();
                    pattern_names(p, &mut ids);
                    ids.into_iter().next()
                }),
                _ => None,
            })
        } else {
            None
        };
        if let Some(p) = param {
            self.frames.push((p, names));
            syn::visit::visit_expr_method_call(self, e);
            self.frames.pop();
        } else {
            syn::visit::visit_expr_method_call(self, e);
        }
    }

    fn visit_expr_binary(&mut self, e: &'ast syn::ExprBinary) {
        let comparison = matches!(
            e.op,
            syn::BinOp::Lt(_)
                | syn::BinOp::Le(_)
                | syn::BinOp::Gt(_)
                | syn::BinOp::Ge(_)
                | syn::BinOp::Eq(_)
                | syn::BinOp::Ne(_)
        );
        if comparison {
            for side in [&*e.left, &*e.right] {
                if let syn::Expr::MethodCall(m) = side {
                    if m.method == "len" {
                        self.bound(&m.receiver);
                    }
                }
            }
        }
        syn::visit::visit_expr_binary(self, e);
    }

    fn visit_expr_assign(&mut self, e: &'ast syn::ExprAssign) {
        self.bound(&e.left);
        syn::visit::visit_expr_assign(self, e);
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*e.func {
            let mem = p.path.segments.last().is_some_and(|s| {
                matches!(s.ident.to_string().as_str(), "take" | "replace" | "swap")
            });
            if mem {
                for a in &e.args {
                    self.bound(a);
                }
            }
        }
        syn::visit::visit_expr_call(self, e);
    }
}

fn read(decls: &[Decl], walk: &Walk) -> Vec<(String, u32, String, String)> {
    let mut out = BTreeMap::new();
    for d in decls {
        let Some(lines) = walk.grown.get(&d.name) else {
            continue;
        };
        if walk.bounded.contains(&d.name) {
            continue;
        }
        let subject = match &d.owner {
            Some(o) => format!("{o}::{}", d.name),
            None => d.name.clone(),
        };
        let mut shown: Vec<String> = lines.iter().take(3).map(ToString::to_string).collect();
        shown.sort();
        let summary = format!(
            "`{subject}` is a `{}` that is added to at {} site(s) (first at line {}) and that nothing in the repository removes from, evicts, clears, reassigns or compares by length",
            d.kind,
            lines.len(),
            shown.first().cloned().unwrap_or_default(),
        );
        out.insert(
            (d.file.clone(), d.line, subject.clone()),
            (d.file.clone(), d.line, subject, summary),
        );
    }
    out.into_values().collect()
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = production_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source outside tests, tooling and vendored trees".into(),
        );
    }
    let mut unparsed: Vec<&str> = Vec::new();
    let mut oversize: Vec<&str> = Vec::new();
    let mut parsed = 0usize;
    let mut walk = Walk::default();
    for path in &files {
        let Some(text) = view.read(path) else {
            unparsed.push(path);
            continue;
        };
        if text.len() > MAX_BYTES {
            oversize.push(path);
            continue;
        }
        let Ok(file) = syn::parse_file(&text) else {
            unparsed.push(path);
            continue;
        };
        parsed += 1;
        walk.file.clone_from(path);
        walk.visit_file(&file);
    }
    let total = files.len();
    let candidates = walk.decls.len();
    let out: Vec<Finding> = read(&walk.decls, &walk)
        .into_iter()
        .map(|(path, line, subject, summary)| {
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::Low,
                view.name(),
                summary,
                "Add an eviction policy or a hard capacity (a size check before the insert, `retain` on a schedule, an LRU type), and decide what happens when the limit is hit. If the growth is bounded by something the rule cannot see, allowlist the subject with the reason.",
            )
            .location(Location::new(path.as_str(), Some(line)))
            .subject(subject)
            .positive_control(format!(
                "{total} production Rust file(s) tracked, {parsed} of them parsed; {candidates} candidate collection(s) found (statics, and cache- or memo-named fields); this one is in {path}"
            ))
        })
        .collect();
    let mut notes = Vec::new();
    if !unparsed.is_empty() {
        notes.push(format!(
            "{} Rust file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            list(&unparsed)
        ));
    }
    if !oversize.is_empty() {
        notes.push(format!(
            "{} Rust file(s) over 2 MiB were not read: {}",
            oversize.len(),
            list(&oversize)
        ));
    }
    if notes.is_empty() {
        Verdict::Findings(out)
    } else {
        Verdict::Noted(out, notes)
    }
}

/// Up to five paths, comma separated.
fn list(paths: &[&str]) -> String {
    paths.iter().take(5).copied().collect::<Vec<_>>().join(", ")
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
    fn a_static_collection_that_only_grows_is_reported() {
        let f = found("use std::collections::HashMap;\nuse std::sync::{LazyLock, Mutex};\nstatic CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));\npub fn put(k: String, v: String) {\n    CACHE.lock().unwrap().insert(k, v);\n}\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("CACHE"));
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(3));
        assert!(f[0].summary.contains("HashMap"), "{}", f[0].summary);
    }

    #[test]
    fn a_static_that_is_never_grown_is_not_reported() {
        let src = "static NAMES: Vec<&str> = Vec::new();\nstatic CACHE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());\npub fn n() -> usize { NAMES.len() + CACHE.lock().unwrap().len() }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_thread_local_read_through_with_is_followed() {
        let grows = "use std::cell::RefCell;\nuse std::collections::HashMap;\nthread_local! {\n    static SEEN: RefCell<HashMap<u64, u64>> = RefCell::new(HashMap::new());\n}\npub fn note(k: u64) {\n    SEEN.with(|s| { s.borrow_mut().insert(k, k); });\n}\n";
        let f = found(grows);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("SEEN"));
        let clears =
            format!("{grows}pub fn reset() {{ SEEN.with(|s| s.borrow_mut().clear()); }}\n");
        assert!(found(&clears).is_empty(), "{:?}", found(&clears));
    }

    #[test]
    fn a_cache_named_field_and_a_field_of_a_cache_struct_are_candidates() {
        let named = "use std::collections::HashMap;\npub struct S { pub response_cache: HashMap<String, String>, pub other: Vec<u8> }\nimpl S { pub fn put(&mut self, k: String) { self.response_cache.insert(k.clone(), k); self.other.push(1); } }\n";
        let f = found(named);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("S::response_cache"));
        let owner = "pub struct ResponseCache { entries: Vec<String> }\nimpl ResponseCache { pub fn put(&mut self, k: String) { self.entries.push(k); } }\n";
        let g = found(owner);
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!(g[0].subject.as_deref(), Some("ResponseCache::entries"));
    }

    #[test]
    fn memory_is_not_a_memo_and_memoize_is() {
        let memory = "pub struct InMemoryStore { sessions: Vec<u8>, memory_log: Vec<u8> }
impl InMemoryStore { pub fn put(&mut self) { self.sessions.push(1); self.memory_log.push(1); } }
";
        assert!(found(memory).is_empty(), "{:?}", found(memory));
        let memo = "pub struct S { memo_table: Vec<u8>, memoized: Vec<u8> }
impl S { pub fn put(&mut self) { self.memo_table.push(1); self.memoized.push(1); } }
";
        assert_eq!(found(memo).len(), 2, "{:?}", found(memo));
    }

    #[test]
    fn a_field_that_is_not_cache_like_is_not_a_candidate() {
        let src = "pub struct S { items: Vec<u8> }\nimpl S { pub fn put(&mut self) { self.items.push(1); } }\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn every_way_to_shrink_or_bound_clears_the_candidate() {
        for body in [
            "self.cache.remove(&1);",
            "self.cache.clear();",
            "self.cache.retain(|_, v| *v > 0);",
            "self.cache.pop_first();",
            "self.cache.drain();",
            "if self.cache.len() >= 100 { return; }",
            "self.cache = HashMap::new();",
            "let _old = std::mem::take(&mut self.cache);",
        ] {
            let src = format!(
                "use std::collections::HashMap;
pub struct S {{ cache: HashMap<u8, u8> }}
impl S {{
    pub fn put(&mut self) {{ self.cache.insert(1, 1); }}
    pub fn other(&mut self) {{ {body} }}
}}
"
            );
            assert!(found(&src).is_empty(), "{body}: {:?}", found(&src));
        }
        let unbounded = "use std::collections::HashMap;
pub struct S { cache: HashMap<u8, u8> }
impl S {
    pub fn put(&mut self) { self.cache.insert(1, 1); }
    pub fn size(&self) -> usize { self.cache.len() }
}
";
        assert_eq!(
            found(unbounded).len(),
            1,
            "a bare len() read is not a bound"
        );
    }

    #[test]
    fn an_evicting_method_on_the_collection_clears_the_candidate() {
        let src = "pub struct Lru;\nimpl Lru { pub fn evict_oldest(&mut self) {} pub fn put(&mut self, _: u8) {} }\npub struct S { cache: Lru, hot_cache: Vec<u8> }\nimpl S {\n    pub fn put(&mut self) { self.hot_cache.push(1); }\n    pub fn tidy(&mut self) { self.hot_cache.evict_expired(); }\n}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_bound_through_a_binding_is_followed() {
        let grows = "use std::collections::HashMap;\nuse std::sync::{LazyLock, Mutex};\nstatic CACHE: LazyLock<Mutex<HashMap<u8, u8>>> = LazyLock::new(|| Mutex::new(HashMap::new()));\npub fn put() { CACHE.lock().unwrap().insert(1, 1); }\n";
        let by_binding =
            format!("{grows}pub fn trim() {{ let mut g = CACHE.lock().unwrap(); g.clear(); }}\n");
        assert!(found(&by_binding).is_empty(), "{:?}", found(&by_binding));
        let grown_by_binding = "use std::collections::HashMap;\nuse std::sync::{LazyLock, Mutex};\nstatic CACHE: LazyLock<Mutex<HashMap<u8, u8>>> = LazyLock::new(|| Mutex::new(HashMap::new()));\npub fn put() { let mut g = CACHE.lock().unwrap(); g.insert(1, 1); }\n";
        assert_eq!(found(grown_by_binding).len(), 1);
    }

    #[test]
    fn a_type_that_bounds_itself_and_test_code_are_not_read() {
        let src = "pub struct S { cache: lru::LruCache<u8, u8>, memo: std::sync::atomic::AtomicUsize }\nimpl S { pub fn put(&mut self) { self.cache.put(1, 1); } }\n#[cfg(test)]\nmod t {\n    static CACHE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());\n    fn f() { CACHE.lock().unwrap().push(1); }\n}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_bound_through_a_parameter_in_another_file_is_a_documented_miss() {
        let v = RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "pub mod b;\nstatic CACHE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());\npub fn put() { CACHE.lock().unwrap().push(1); }\n"),
            ("src/b.rs", "pub fn trim(c: &std::sync::Mutex<Vec<u8>>) { c.lock().unwrap().clear(); }\n"),
        ]);
        // the bound is on the parameter `c`, not on `CACHE`: names are matched by identifier, so the candidate is still reported
        match judge(&v) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => assert_eq!(f.len(), 1),
            Verdict::NotApplicable(r) => panic!("{r}"),
        }
    }

    #[test]
    fn every_finding_validates() {
        for f in found("static CACHE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());\npub fn put() { CACHE.lock().unwrap().push(1); }\n") {
            f.validate().unwrap();
        }
    }
}
