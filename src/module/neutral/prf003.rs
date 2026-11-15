// SPDX-License-Identifier: MIT OR Apache-2.0
//! PRF-003: code avoids the shapes that signal accidental super-linear cost.
//!
//! **What this judges (Rust, from the source alone):** every tracked production `.rs` file (outside vendored directories, tests,
//! examples, tools, `build.rs`, files over 2 MiB, and outside `#[cfg(test)]` and `#[test]` items) is parsed with `syn`. Per function,
//! **one Medium finding at low confidence**, at the first site, with the sites listed, for four shapes read **inside a loop** (a `for`,
//! `while` or `loop`, or the closure given to an iterator adaptor such as `map`, `for_each` or `filter` on an iterator):
//!
//! 1. **Nested scan:** a linear search (`contains`, or `iter().any`, `iter().find`, `iter().position`, `iter().rposition`) of a
//!    `Vec`, `VecDeque`, slice or array that is declared **outside** the loop. The collection's type is known only from a declaration
//!    the rule can read: a typed parameter, a `let` with a type or with a constructor (`Vec::new()`, `vec![]`, `.collect::<Vec<_>>()`,
//!    `.to_vec()`), or a field of the same file whose declared type is one of those (`self.items`).
//! 2. **Accumulating concatenation:** `s = format!(".. {s} ..")` where `s` is declared outside the loop and the macro's tokens name it,
//!    which copies the whole string on every pass.
//! 3. **Query per row:** a `query`, `query_as`, `query_scalar`, `query_row`, `query_one`, `query_opt`, `query_map`, `query_and_then`,
//!    `simple_query`, `prepare` or `prepare_cached` call with a string literal that starts with `SELECT` (any case).
//! 4. **Copy made only to be read:** `.clone()`, `.to_vec()` or `.to_owned()` of a `Vec`, `VecDeque`, slice, `String`, `HashMap`,
//!    `HashSet`, `BTreeMap`, `BTreeSet` or `BinaryHeap` whose type is known as above and that is declared **outside** the loop, when
//!    the copy is only read (`.iter()`, `.len()`, `.is_empty()`, `.contains()`, `.get()`, `.first()`, `.last()`, `.join()`, `.chars()`
//!    and the like called on it) or is what a `for` inside another loop iterates. A clone handed on (pushed, returned, put in a
//!    struct) is not read: the loop body may need its own copy.
//!
//! The closures given to `map_err`, `ok_or_else`, `unwrap_or_else` and `or_else` run on a failure or default path, not once per
//! element, and are not read.
//!
//! A finding is a cue to read, never a verdict: a clone the loop body needs a fresh copy of, or a scan of a three-element list, is fine.
//!
//! **What it cannot see:** a type the rule cannot read from a declaration (a collection returned by a function, an inferred closure
//! parameter, a field of another file), a `HashSet` or `HashMap` called `contains` (they are not in the scanned set, so a
//! known-hash type is never reported), a search hidden in a called function, an `INSERT`, `UPDATE` or `DELETE` per row (batching
//! applies, but the rule reads `SELECT` only), string building with `+` (amortised in Rust), whether the loop is large, and macros.
//! A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_src::{is_test_item, line_of, production_files, MAX_BYTES};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashMap;
use syn::visit::Visit;

const ID: &str = crate::rule_id::PRF_003;

/// What a declaration tells the rule about a binding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Kind {
    /// A `Vec`, `VecDeque`, slice or array: searched by walking it.
    seq: bool,
    /// A collection or string whose clone copies every element.
    heavy: bool,
}

const SEQ_HEAVY: Kind = Kind {
    seq: true,
    heavy: true,
};
const HEAVY: Kind = Kind {
    seq: false,
    heavy: true,
};

fn kind_of_name(name: &str) -> Option<Kind> {
    match name {
        "Vec" | "VecDeque" => Some(SEQ_HEAVY),
        "String" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet" | "BinaryHeap" => Some(HEAVY),
        _ => None,
    }
}

fn kind_of_type(ty: &syn::Type) -> Option<Kind> {
    match ty {
        syn::Type::Reference(r) => kind_of_type(&r.elem),
        syn::Type::Paren(p) => kind_of_type(&p.elem),
        syn::Type::Group(g) => kind_of_type(&g.elem),
        syn::Type::Slice(_) => Some(SEQ_HEAVY),
        syn::Type::Array(_) => Some(Kind {
            seq: true,
            heavy: false,
        }),
        syn::Type::Path(p) if p.qself.is_none() => {
            kind_of_name(&p.path.segments.last()?.ident.to_string())
        }
        _ => None,
    }
}

/// What an initialiser says its binding is: a constructor, `vec![]`, `format!`, `.to_vec()`, `.collect::<Vec<_>>()`.
fn kind_of_init(e: &syn::Expr) -> Option<Kind> {
    match e {
        syn::Expr::Paren(p) => kind_of_init(&p.expr),
        syn::Expr::Macro(m) => match m.mac.path.get_ident()?.to_string().as_str() {
            "vec" => Some(SEQ_HEAVY),
            "format" => Some(HEAVY),
            _ => None,
        },
        syn::Expr::Call(c) => {
            let syn::Expr::Path(p) = &*c.func else {
                return None;
            };
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segs.len() == 2 && matches!(segs[1].as_str(), "new" | "with_capacity" | "from") {
                return kind_of_name(&segs[0]);
            }
            None
        }
        syn::Expr::MethodCall(m) => match m.method.to_string().as_str() {
            "to_vec" => Some(SEQ_HEAVY),
            "to_string" => Some(HEAVY),
            "collect" => {
                let t = m.turbofish.as_ref()?.args.first()?;
                match t {
                    syn::GenericArgument::Type(t) => kind_of_type(t),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
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

/// The names of a pattern's bindings.
fn pattern_names(p: &syn::Pat, out: &mut Vec<String>) {
    match p {
        syn::Pat::Ident(i) => out.push(i.ident.to_string()),
        syn::Pat::TupleStruct(t) => t.elems.iter().for_each(|e| pattern_names(e, out)),
        syn::Pat::Tuple(t) => t.elems.iter().for_each(|e| pattern_names(e, out)),
        syn::Pat::Reference(r) => pattern_names(&r.pat, out),
        syn::Pat::Type(t) => pattern_names(&t.pat, out),
        syn::Pat::Struct(s) => s.fields.iter().for_each(|f| pattern_names(&f.pat, out)),
        _ => {}
    }
}

/// The binding an expression stands for: `name`, or `self.name` for a field of `self`.
fn key_of(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Path(p) if p.qself.is_none() => p.path.get_ident().map(ToString::to_string),
        syn::Expr::Field(f) => {
            let syn::Member::Named(name) = &f.member else {
                return None;
            };
            let syn::Expr::Path(p) = &*f.base else {
                return None;
            };
            p.path.is_ident("self").then(|| format!("self.{name}"))
        }
        syn::Expr::Paren(p) => key_of(&p.expr),
        syn::Expr::Reference(r) => key_of(&r.expr),
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => key_of(&u.expr),
        _ => None,
    }
}

/// Whether the tokens name the identifier anywhere, nested groups included.
fn mentions(tokens: proc_macro2::TokenStream, name: &str) -> bool {
    tokens.into_iter().any(|t| match t {
        proc_macro2::TokenTree::Ident(i) => i == name,
        proc_macro2::TokenTree::Group(g) => mentions(g.stream(), name),
        _ => false,
    })
}

/// A method that walks an iterator, so the closure it is given runs once per element.
const ITER_SOURCES: &[&str] = &[
    "iter",
    "iter_mut",
    "into_iter",
    "chars",
    "bytes",
    "lines",
    "keys",
    "values",
    "values_mut",
    "drain",
    "windows",
    "chunks",
    "split",
    "split_whitespace",
    "char_indices",
];

const ADAPTORS: &[&str] = &[
    "for_each",
    "map",
    "filter",
    "filter_map",
    "flat_map",
    "any",
    "all",
    "position",
    "find",
    "find_map",
    "fold",
    "inspect",
    "take_while",
    "skip_while",
    "try_for_each",
    "max_by_key",
    "min_by_key",
    "partition",
];

const QUERIES: &[&str] = &[
    "query",
    "query_as",
    "query_scalar",
    "query_row",
    "query_one",
    "query_opt",
    "query_map",
    "query_and_then",
    "simple_query",
    "prepare",
    "prepare_cached",
];

/// The closures these take run on a failure or default path, once, not per element.
const COLD: &[&str] = &["map_err", "ok_or_else", "unwrap_or_else", "or_else"];

/// What a copy is used for when it is only looked at.
const READS: &[&str] = &[
    "iter",
    "len",
    "is_empty",
    "contains",
    "get",
    "first",
    "last",
    "join",
    "chars",
    "bytes",
    "lines",
    "starts_with",
    "ends_with",
    "as_str",
    "trim",
    "keys",
    "values",
];

fn runs_per_element(receiver: &syn::Expr) -> bool {
    match receiver {
        syn::Expr::MethodCall(m) => {
            ITER_SOURCES.contains(&m.method.to_string().as_str()) || runs_per_element(&m.receiver)
        }
        syn::Expr::Paren(p) => matches!(&*p.expr, syn::Expr::Range(_)) || runs_per_element(&p.expr),
        syn::Expr::Range(_) => true,
        _ => false,
    }
}

struct Site {
    line: u32,
    text: String,
}

struct Body<'a> {
    /// A binding to what its declaration says and the loop depth it was declared at.
    bindings: HashMap<String, (Kind, usize)>,
    fields: &'a HashMap<String, Option<Kind>>,
    depth: usize,
    /// How many failure-path closures the walk is inside.
    cold: usize,
    sites: Vec<Site>,
}

impl Body<'_> {
    fn declare(&mut self, name: String, kind: Option<Kind>) {
        match kind {
            Some(k) => {
                self.bindings.insert(name, (k, self.depth));
            }
            None => {
                self.bindings.remove(&name);
            }
        }
    }

    /// What a binding is, when it was declared outside the loop being read.
    fn outer(&self, e: &syn::Expr) -> Option<(String, Kind)> {
        let key = key_of(e)?;
        if let Some(field) = key.strip_prefix("self.") {
            let kind = self.fields.get(field).copied().flatten()?;
            return Some((key, kind));
        }
        let (kind, depth) = *self.bindings.get(&key)?;
        (depth < self.depth).then_some((key, kind))
    }

    fn site(&mut self, span: proc_macro2::Span, text: String) {
        if self.cold > 0 {
            return;
        }
        self.sites.push(Site {
            line: line_of(span),
            text,
        });
    }

    fn in_loop<F: FnOnce(&mut Self)>(&mut self, params: &[&syn::Pat], f: F) {
        self.depth += 1;
        for p in params {
            let mut names = Vec::new();
            pattern_names(p, &mut names);
            for n in names {
                self.declare(n, None);
            }
        }
        f(self);
        self.depth -= 1;
    }
}

impl<'ast> Visit<'ast> for Body<'_> {
    // a function or item inside the body is read on its own
    fn visit_item(&mut self, _: &'ast syn::Item) {}

    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let Some(init) = &l.init {
            self.visit_expr(&init.expr);
        }
        let mut names = Vec::new();
        pattern_names(&l.pat, &mut names);
        let declared = match &l.pat {
            syn::Pat::Type(t) => kind_of_type(&t.ty),
            _ => None,
        };
        let kind = declared.or_else(|| l.init.as_ref().and_then(|i| kind_of_init(&i.expr)));
        let single = names.len() == 1;
        for n in names {
            self.declare(n, if single { kind } else { None });
        }
    }

    fn visit_expr_for_loop(&mut self, e: &'ast syn::ExprForLoop) {
        if self.depth > 0 {
            if let Some((key, method, span)) = self.copy_of_outer(&e.expr) {
                self.site(
                    span,
                    format!("a `for` iterates over `{key}.{method}()`, a fresh copy on every pass of the outer loop"),
                );
            }
        }
        self.visit_expr(&e.expr);
        self.in_loop(&[&e.pat], |s| s.visit_block(&e.body));
    }

    fn visit_expr_while(&mut self, e: &'ast syn::ExprWhile) {
        self.in_loop(&[], |s| {
            s.visit_expr(&e.cond);
            s.visit_block(&e.body);
        });
    }

    fn visit_expr_loop(&mut self, e: &'ast syn::ExprLoop) {
        self.in_loop(&[], |s| s.visit_block(&e.body));
    }

    fn visit_expr_assign(&mut self, e: &'ast syn::ExprAssign) {
        if self.depth > 0 {
            if let (Some(key), syn::Expr::Macro(m)) = (key_of(&e.left), &*e.right) {
                let is_format = m.mac.path.is_ident("format");
                if is_format && !key.contains('.') && mentions(m.mac.tokens.clone(), &key) {
                    if let Some((_, k)) = self.outer(&e.left) {
                        if k.heavy {
                            self.site(
                                syn::spanned::Spanned::span(&*e.right),
                                format!(
                                    "`{key} = format!(..{key}..)` copies the string on every pass"
                                ),
                            );
                        }
                    }
                }
            }
        }
        syn::visit::visit_expr_assign(self, e);
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if self.depth > 0 {
            if let syn::Expr::Path(p) = &*e.func {
                let last = p.path.segments.last().map(|s| s.ident.to_string());
                if last.as_deref().is_some_and(|n| QUERIES.contains(&n)) && select_literal(&e.args)
                {
                    self.site(
                        syn::spanned::Spanned::span(e),
                        "a `SELECT` is issued once per pass".into(),
                    );
                }
            }
        }
        syn::visit::visit_expr_call(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        let method = e.method.to_string();
        if self.depth > 0 {
            self.method_site(e, &method);
        }
        self.visit_expr(&e.receiver);
        let per_element = ADAPTORS.contains(&method.as_str()) && runs_per_element(&e.receiver);
        let cold = COLD.contains(&method.as_str());
        if cold {
            self.cold += 1;
        }
        for a in &e.args {
            match a {
                syn::Expr::Closure(c) if per_element => {
                    let params: Vec<&syn::Pat> = c.inputs.iter().collect();
                    self.in_loop(&params, |s| s.visit_expr(&c.body));
                }
                other => self.visit_expr(other),
            }
        }
        if cold {
            self.cold -= 1;
        }
    }
}

impl Body<'_> {
    /// `X.clone()`, `X.to_vec()` or `X.to_owned()` where `X` is a heavy binding declared outside the loop: the name, the method and its span.
    fn copy_of_outer(&self, e: &syn::Expr) -> Option<(String, String, proc_macro2::Span)> {
        let syn::Expr::MethodCall(m) = e else {
            return None;
        };
        let method = m.method.to_string();
        if !matches!(method.as_str(), "clone" | "to_vec" | "to_owned") || !m.args.is_empty() {
            return None;
        }
        let (key, kind) = self.outer(&m.receiver)?;
        kind.heavy.then(|| (key, method, m.method.span()))
    }

    fn method_site(&mut self, e: &syn::ExprMethodCall, method: &str) {
        let span = e.method.span();
        match method {
            "contains" => {
                if let Some((key, k)) = self.outer(&e.receiver) {
                    if k.seq {
                        self.site(
                            span,
                            format!(
                                "`{key}.contains(..)` walks the whole collection on every pass"
                            ),
                        );
                    }
                }
            }
            "any" | "find" | "position" | "rposition" => {
                if let syn::Expr::MethodCall(inner) = &*e.receiver {
                    if inner.method == "iter" {
                        if let Some((key, k)) = self.outer(&inner.receiver) {
                            if k.seq {
                                self.site(
                                    span,
                                    format!("`{key}.iter().{method}(..)` walks the whole collection on every pass"),
                                );
                            }
                        }
                    }
                }
            }
            m if READS.contains(&m) => {
                if let Some((key, copy, copy_span)) = self.copy_of_outer(&e.receiver) {
                    self.site(
                        copy_span,
                        format!("`{key}.{copy}().{m}(..)` copies the whole collection on every pass only to read it"),
                    );
                }
            }
            m if QUERIES.contains(&m) && select_literal(&e.args) => {
                self.site(span, "a `SELECT` is issued once per pass".into());
            }
            _ => {}
        }
    }
}

fn select_literal(args: &syn::punctuated::Punctuated<syn::Expr, syn::Token![,]>) -> bool {
    args.iter().any(|a| match a {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => s.value().trim_start().to_lowercase().starts_with("select"),
            _ => false,
        },
        _ => false,
    })
}

/// The collections the struct fields of one file declare, by field name; a name declared with two different kinds is unknown.
#[derive(Default)]
struct Fields {
    kinds: HashMap<String, Option<Kind>>,
}

impl<'ast> Visit<'ast> for Fields {
    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if let syn::Fields::Named(fields) = &i.fields {
            for f in &fields.named {
                let Some(name) = f.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                let kind = kind_of_type(&f.ty);
                self.kinds
                    .entry(name)
                    .and_modify(|k| {
                        if *k != kind {
                            *k = None;
                        }
                    })
                    .or_insert(kind);
            }
        }
        syn::visit::visit_item_struct(self, i);
    }
}

struct Hit {
    line: u32,
    subject: String,
    summary: String,
}

struct Walk<'a> {
    fields: &'a HashMap<String, Option<Kind>>,
    hits: Vec<Hit>,
    owner: Option<String>,
}

impl Walk<'_> {
    fn function(&mut self, sig: &syn::Signature, block: &syn::Block, attrs: &[syn::Attribute]) {
        if is_test_item(attrs) {
            return;
        }
        let mut body = Body {
            bindings: HashMap::new(),
            fields: self.fields,
            depth: 0,
            cold: 0,
            sites: Vec::new(),
        };
        for a in &sig.inputs {
            if let syn::FnArg::Typed(t) = a {
                let mut names = Vec::new();
                pattern_names(&t.pat, &mut names);
                if let [only] = names.as_slice() {
                    body.declare(only.clone(), kind_of_type(&t.ty));
                }
            }
        }
        body.visit_block(block);
        if body.sites.is_empty() {
            return;
        }
        body.sites.sort_by_key(|s| s.line);
        let name = sig.ident.to_string();
        let subject = match &self.owner {
            Some(t) => format!("{t}::{name}"),
            None => name,
        };
        let shown: Vec<String> = body
            .sites
            .iter()
            .take(4)
            .map(|s| format!("line {}: {}", s.line, s.text))
            .collect();
        let more = body.sites.len().saturating_sub(4);
        self.hits.push(Hit {
            line: body.sites[0].line,
            summary: format!(
                "`{subject}` has {} algorithmic red flag(s) inside a loop ({}{})",
                body.sites.len(),
                shown.join("; "),
                if more > 0 {
                    format!("; and {more} more")
                } else {
                    String::new()
                },
            ),
            subject,
        });
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_test_item(&i.attrs) {
            return;
        }
        let owner = self.owner.take();
        self.function(&i.sig, &i.block, &i.attrs);
        syn::visit::visit_item_fn(self, i);
        self.owner = owner;
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_test_item(&i.attrs) {
            return;
        }
        let owner = self.owner.replace(type_name(&i.self_ty));
        for item in &i.items {
            if let syn::ImplItem::Fn(f) = item {
                self.function(&f.sig, &f.block, &f.attrs);
            }
        }
        // a function declared inside a method body is reached through the default walk, owned by no type
        self.owner = None;
        syn::visit::visit_item_impl(self, i);
        self.owner = owner;
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if is_test_item(&i.attrs) {
            return;
        }
        let owner = self.owner.replace(i.ident.to_string());
        for item in &i.items {
            if let syn::TraitItem::Fn(f) = item {
                if let Some(block) = &f.default {
                    self.function(&f.sig, block, &f.attrs);
                }
            }
        }
        self.owner = None;
        syn::visit::visit_item_trait(self, i);
        self.owner = owner;
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = production_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source outside tests, tooling and vendored trees".into(),
        );
    }
    let mut out = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let mut oversize: Vec<&str> = Vec::new();
    let mut parsed = 0usize;
    let total = files.len();
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
        let mut fields = Fields::default();
        fields.visit_file(&file);
        let mut walk = Walk {
            fields: &fields.kinds,
            hits: Vec::new(),
            owner: None,
        };
        walk.visit_file(&file);
        for h in walk.hits {
            out.push((path.clone(), h));
        }
    }
    out.sort_by(|a, b| (&a.0, a.1.line).cmp(&(&b.0, b.1.line)));
    let findings: Vec<Finding> = out
        .into_iter()
        .map(|(path, h)| {
            Finding::new(
                ID,
                Severity::Medium,
                Confidence::Low,
                view.name(),
                h.summary,
                "Replace the pattern with an indexed lookup (a `HashSet` or `HashMap` built once), a single pass, a builder (`push_str` or `write!` into one `String`), a copy hoisted out of the loop, or one batched query. If the collection is small or the copy is needed, allowlist the subject with the reason.",
            )
            .location(Location::new(path.as_str(), Some(h.line)))
            .subject(h.subject)
            .positive_control(format!(
                "{total} production Rust file(s) tracked, {parsed} of them parsed and walked for loops; this function is in {path}"
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
        Verdict::Findings(findings)
    } else {
        Verdict::Noted(findings, notes)
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
    fn a_contains_on_an_outer_vec_inside_a_loop_is_reported() {
        let f = found("pub fn dups(a: &[u32], seen: Vec<u32>) -> usize {\n    let mut n = 0;\n    for x in a {\n        if seen.contains(x) { n += 1; }\n    }\n    n\n}\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
        assert_eq!(f[0].subject.as_deref(), Some("dups"));
        assert!(f[0].summary.contains("seen.contains"), "{}", f[0].summary);
    }

    #[test]
    fn a_hash_set_a_vec_declared_inside_the_loop_and_the_loop_variable_are_not_reported() {
        let src = "use std::collections::HashSet;\npub fn f(a: &[u32], seen: HashSet<u32>, rows: Vec<Vec<u32>>) -> usize {\n    let mut n = 0;\n    for x in a {\n        if seen.contains(x) { n += 1; }\n        let local = vec![1, 2, 3];\n        if local.contains(x) { n += 1; }\n    }\n    for row in &rows { if row.contains(&1) { n += 1; } }\n    n\n}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn an_iter_scan_and_a_scan_in_an_iterator_closure_are_reported() {
        let any = "pub fn f(a: &[u32], b: Vec<u32>) -> bool { for x in a { if b.iter().any(|y| y == x) { return true; } } false }\n";
        assert_eq!(found(any).len(), 1, "{:?}", found(any));
        let pos = "pub fn f(a: &[u32], b: Vec<u32>) -> Vec<Option<usize>> { a.iter().map(|x| b.iter().position(|y| y == x)).collect() }\n";
        assert_eq!(found(pos).len(), 1, "{:?}", found(pos));
        let option_map =
            "pub fn f(a: Option<u32>, b: Vec<u32>) -> Option<bool> { a.map(|x| b.contains(&x)) }\n";
        assert!(found(option_map).is_empty(), "an Option::map is not a loop");
    }

    #[test]
    fn a_field_with_a_declared_collection_type_is_followed() {
        let src = "pub struct S { items: Vec<u32>, tags: std::collections::HashSet<u32> }\nimpl S {\n    pub fn f(&self, a: &[u32]) -> usize {\n        let mut n = 0;\n        for x in a { if self.items.contains(x) { n += 1; } if self.tags.contains(x) { n += 1; } }\n        n\n    }\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("S::f"));
        assert!(f[0].summary.contains("self.items"), "{}", f[0].summary);
    }

    #[test]
    fn a_string_rebuilt_from_itself_with_format_is_reported_and_push_str_is_not() {
        let bad = "pub fn f(parts: &[&str]) -> String {\n    let mut s = String::new();\n    for p in parts { s = format!(\"{}{}\", s, p); }\n    s\n}\n";
        let f = found(bad);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("format!"), "{}", f[0].summary);
        let good = "pub fn f(parts: &[&str]) -> String {\n    let mut s = String::new();\n    for p in parts { s.push_str(p); s += \"x\"; }\n    let t = format!(\"{}\", s);\n    t\n}\n";
        assert!(found(good).is_empty(), "{:?}", found(good));
    }

    #[test]
    fn a_select_inside_a_loop_is_reported_and_one_outside_is_not() {
        let bad = "pub fn f(ids: &[u32], conn: &Conn) { for id in ids { let _ = conn.query_row(\"SELECT name FROM t WHERE id = ?\", [id]); } }\npub struct Conn;\nimpl Conn { pub fn query_row(&self, _: &str, _: [&u32; 1]) {} }\n";
        assert_eq!(found(bad).len(), 1, "{:?}", found(bad));
        let function_form =
            "pub fn f(ids: &[u32]) { for _ in ids { let _ = sqlx::query(\" select 1 \"); } }\n";
        assert_eq!(found(function_form).len(), 1);
        let outside = "pub fn f(conn: &Conn) { let _ = conn.query_row(\"SELECT 1\", []); }\npub struct Conn;\nimpl Conn { pub fn query_row(&self, _: &str, _: [u8; 0]) {} }\n";
        assert!(found(outside).is_empty());
        let insert = "pub fn f(ids: &[u32], conn: &Conn) { for _ in ids { conn.query_row(\"INSERT INTO t VALUES (1)\", []); } }\npub struct Conn;\nimpl Conn { pub fn query_row(&self, _: &str, _: [u8; 0]) {} }\n";
        assert!(found(insert).is_empty(), "an INSERT is not read");
    }

    #[test]
    fn a_copy_made_only_to_be_read_inside_a_loop_is_reported() {
        let bad = "pub fn f(names: Vec<String>, n: usize) -> usize {\n    let mut t = 0;\n    for _ in 0..n { t += names.clone().len(); }\n    t\n}\n";
        let f = found(bad);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].summary.contains("names.clone().len"),
            "{}",
            f[0].summary
        );
        let iterate = "pub fn f(names: Vec<String>, outer: &[u32]) -> usize {\n    let mut t = 0;\n    for _ in outer { for s in names.clone() { t += s.len(); } }\n    t\n}\n";
        assert_eq!(found(iterate).len(), 1, "{:?}", found(iterate));
        let top_level = "pub fn f(names: Vec<String>) -> usize { let mut t = 0; for s in names.clone() { t += s.len(); } t }\n";
        assert!(
            found(top_level).is_empty(),
            "a single loop over a copy is not a per-pass copy"
        );
    }

    #[test]
    fn a_clone_that_is_handed_on_or_made_on_a_failure_path_is_not_reported() {
        let handed = "pub fn f(names: Vec<String>, n: usize) -> Vec<Vec<String>> {\n    let mut out = Vec::new();\n    for _ in 0..n { out.push(names.clone()); }\n    out\n}\n";
        assert!(found(handed).is_empty(), "{:?}", found(handed));
        let string = "pub fn f(why: String, n: &[u32]) -> Vec<String> {\n    let mut r = Vec::new();\n    for _ in n { r.push(why.clone()); }\n    r\n}\n";
        assert!(found(string).is_empty());
        let cold = "pub fn f(done: Vec<String>, n: &[u32]) -> Result<(), usize> {\n    for x in n { g(*x).map_err(|_| done.clone().len())?; }\n    Ok(())\n}\nfn g(_: u32) -> Result<(), ()> { Ok(()) }\n";
        assert!(found(cold).is_empty(), "{:?}", found(cold));
        let shared = "use std::sync::Arc;\npub fn f(shared: Arc<Vec<u8>>, items: Vec<String>) {\n    for i in &items { let _ = shared.clone().len(); let _ = i.clone().len(); }\n}\n";
        assert!(found(shared).is_empty(), "{:?}", found(shared));
    }

    #[test]
    fn test_code_and_a_function_inside_a_function_are_read_on_their_own() {
        let src = "#[cfg(test)]\nmod t { fn f(a: &[u32], b: Vec<u32>) { for x in a { b.contains(x); } } }\npub fn outer(b: Vec<u32>) {\n    fn inner(a: &[u32], b: Vec<u32>) { for x in a { if b.contains(x) {} } }\n    let _ = b;\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].subject.as_deref(), Some("inner"));
    }

    #[test]
    fn several_sites_in_one_function_are_one_finding() {
        let src = "pub fn f(a: &[u32], b: Vec<u32>, c: Vec<u32>) -> usize {\n    let mut n = 0;\n    for x in a { if b.contains(x) { n += 1; } if c.contains(x) { n += 1; } }\n    n\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].summary.contains("2 algorithmic red flag"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f(a: &[u32], b: Vec<u32>) { for x in a { b.contains(x); } }\n") {
            f.validate().unwrap();
        }
    }
}
