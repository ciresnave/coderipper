// SPDX-License-Identifier: MIT OR Apache-2.0
//! `test-artifact-path`: a test that finds a built artifact by walking up from `current_exe()` instead of asking Cargo (Rust).
//!
//! Cargo sets the variables a test needs: `CARGO_BIN_EXE_<name>` (the absolute path of a binary of the same package, for integration tests
//! and benchmarks), `CARGO_MANIFEST_DIR`, `CARGO_TARGET_TMPDIR`, and `OUT_DIR` (for build scripts and the code they feed). A test that
//! instead takes `std::env::current_exe()` and goes up (`parent()`, `ancestors()`, `pop()`, `with_file_name`) or joins a directory it
//! assumes (`"examples"`, `"deps"`, `"target"`, `"debug"`, `"release"`) is hard-coding where Cargo puts things, and breaks when Cargo changes
//! its layout. (Cargo 1.100's build-layout change turned 29 of agentlife's tests red this way.)
//!
//! **What this judges (Rust, from the source alone):** in **test code**, a path derived from `current_exe()`. Test code is a file under
//! `tests/`, `examples/` or `benches/`, a `tests.rs` file, an item under `#[cfg(test)]`, and a `#[test]` function. Inside one function, a
//! local is *derived* when it is bound from an expression that contains a call to `current_exe` (`let exe = std::env::current_exe()?;`,
//! `let mut p = current_exe().unwrap();`). A site is a call on `current_exe()` or on such a local, through any chain of `unwrap`, `expect`,
//! `?`, `ok`, `and_then` and the like, of `parent`, `ancestors`, `pop` or `with_file_name`, or of `join` with a literal naming one of the
//! directories above, and `Path::parent(..)` or `Path::ancestors(..)` applied to one. One finding per function, at its first such site.
//!
//! **Not reported:** `Command::new(current_exe()?)` and the other uses that run the test binary itself or print its path (they derive no
//! sibling path); non-test code (a program finding a sibling executable at run time is a different design, judged by nobody here); and a
//! function that also reads `CARGO_BIN_EXE_`, `CARGO_MANIFEST_DIR`, `CARGO_TARGET_TMPDIR` or `OUT_DIR` (the path is then Cargo's own, the
//! walk is a fallback or a check).
//!
//! **What it cannot see:** the path passed to another function that derives the layout, a `current_exe` stored in a `static` or a struct
//! field, macro-generated code, a helper in another file that does the walk for the test. A file `syn` cannot parse is named in a note,
//! never read as clean.

use super::global_state::{is_test_attr, line_of, vendored};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall, ImplItemFn, ItemFn, ItemImpl, ItemMod, Local, Pat};

const ID: &str = crate::rule_id::TEST_ARTIFACT_PATH;

/// Methods that go up from, or sideways along, a path.
const WALKERS: &[&str] = &["parent", "ancestors", "pop", "with_file_name"];

/// Directory names a test assumes when it joins them onto the walked path.
const ASSUMED_DIRS: &[&str] = &["examples", "deps", "target", "debug", "release"];

/// If a function reads one of these, the path it uses is Cargo's own.
const CARGO_VARS: &[&str] = &[
    "CARGO_BIN_EXE_",
    "CARGO_MANIFEST_DIR",
    "CARGO_TARGET_TMPDIR",
    "OUT_DIR",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub line: u32,
    pub function: String,
    /// What derived the path, for the message.
    pub how: String,
}

/// Whether a path is test code by where it lives.
fn is_test_file(path: &str) -> bool {
    let segs: Vec<&str> = path.split('/').collect();
    let (dirs, file) = segs.split_at(segs.len() - 1);
    dirs.iter()
        .any(|s| matches!(*s, "tests" | "examples" | "benches"))
        || file[0] == "tests.rs"
}

/// Whether `e` is, or is a chain of calls on, `current_exe()` or a local derived from it.
fn rooted(e: &Expr, derived: &HashSet<String>) -> bool {
    match e {
        Expr::MethodCall(m) => rooted(&m.receiver, derived),
        Expr::Try(t) => rooted(&t.expr, derived),
        Expr::Paren(p) => rooted(&p.expr, derived),
        Expr::Group(g) => rooted(&g.expr, derived),
        Expr::Reference(r) => rooted(&r.expr, derived),
        Expr::Await(a) => rooted(&a.base, derived),
        Expr::Call(c) => is_current_exe_call(c),
        Expr::Path(p) => {
            p.path.segments.len() == 1 && derived.contains(&p.path.segments[0].ident.to_string())
        }
        _ => false,
    }
}

fn is_current_exe_call(c: &ExprCall) -> bool {
    matches!(&*c.func, Expr::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "current_exe"))
}

/// Whether an expression contains a call to `current_exe` anywhere.
fn mentions_current_exe(e: &Expr) -> bool {
    struct Find(bool);
    impl<'ast> Visit<'ast> for Find {
        fn visit_expr_call(&mut self, c: &'ast ExprCall) {
            if is_current_exe_call(c) {
                self.0 = true;
            }
            syn::visit::visit_expr_call(self, c);
        }
    }
    let mut f = Find(false);
    f.visit_expr(e);
    f.0
}

fn str_lit(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(l) => match &l.lit {
            syn::Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

/// Reads one function body.
struct FnScan {
    derived: HashSet<String>,
    first: Option<(u32, String)>,
}

impl<'ast> Visit<'ast> for FnScan {
    fn visit_local(&mut self, l: &'ast Local) {
        syn::visit::visit_local(self, l);
        let name = match &l.pat {
            Pat::Ident(p) => Some(p.ident.to_string()),
            Pat::Type(t) => match &*t.pat {
                Pat::Ident(p) => Some(p.ident.to_string()),
                _ => None,
            },
            _ => None,
        };
        if let (Some(name), Some(init)) = (name, &l.init) {
            if mentions_current_exe(&init.expr) || rooted(&init.expr, &self.derived) {
                self.derived.insert(name);
            }
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast ExprMethodCall) {
        let method = m.method.to_string();
        let walks = WALKERS.contains(&method.as_str())
            || (method == "join"
                && m.args
                    .first()
                    .and_then(str_lit)
                    .is_some_and(|d| ASSUMED_DIRS.contains(&d.as_str())));
        if walks && rooted(&m.receiver, &self.derived) && self.first.is_none() {
            self.first = Some((
                line_of(m.method.span()),
                format!("`.{method}(..)` on a path from `current_exe()`"),
            ));
        }
        syn::visit::visit_expr_method_call(self, m);
    }

    fn visit_expr_call(&mut self, c: &'ast ExprCall) {
        if let Expr::Path(p) = &*c.func {
            let last = p.path.segments.last().map(|s| s.ident.to_string());
            if matches!(last.as_deref(), Some("parent" | "ancestors"))
                && p.path.segments.len() > 1
                && c.args.first().is_some_and(|a| rooted(a, &self.derived))
                && self.first.is_none()
            {
                self.first = Some((
                    line_of(c.span()),
                    "`Path::parent(..)` on a path from `current_exe()`".to_string(),
                ));
            }
        }
        syn::visit::visit_expr_call(self, c);
    }
}

struct Scan<'a> {
    lines: Vec<&'a str>,
    file_is_test: bool,
    in_test: bool,
    out: Vec<Site>,
}

impl Scan<'_> {
    fn function(&mut self, name: &str, span: proc_macro2::Span, block: &syn::Block) {
        let mut s = FnScan {
            derived: HashSet::new(),
            first: None,
        };
        s.visit_block(block);
        let Some((line, how)) = s.first else {
            return;
        };
        // Cargo's own variables read in the same function: the path is Cargo's, the walk is a fallback or a check
        let (start, end) = (
            span.start().line.saturating_sub(1),
            span.end().line.min(self.lines.len()),
        );
        let text = self.lines.get(start..end).unwrap_or(&[]).join("\n");
        if CARGO_VARS.iter().any(|v| text.contains(v)) {
            return;
        }
        self.out.push(Site {
            line,
            function: name.to_string(),
            how,
        });
    }

    fn in_scope(&self, attrs: &[syn::Attribute]) -> bool {
        self.file_is_test || self.in_test || attrs.iter().any(is_test_attr)
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        let was = self.in_test;
        if m.attrs.iter().any(is_test_attr) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, m);
        self.in_test = was;
    }

    fn visit_item_impl(&mut self, i: &'ast ItemImpl) {
        let was = self.in_test;
        if i.attrs.iter().any(is_test_attr) {
            self.in_test = true;
        }
        syn::visit::visit_item_impl(self, i);
        self.in_test = was;
    }

    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        if self.in_scope(&f.attrs) {
            self.function(&f.sig.ident.to_string(), f.span(), &f.block);
        }
    }

    fn visit_impl_item_fn(&mut self, f: &'ast ImplItemFn) {
        if self.in_scope(&f.attrs) {
            self.function(&f.sig.ident.to_string(), f.span(), &f.block);
        }
    }
}

pub(super) fn sites(path: &str, text: &str) -> Option<Vec<Site>> {
    let parsed = syn::parse_file(text).ok()?;
    let mut scan = Scan {
        lines: text.lines().collect(),
        file_is_test: is_test_file(path),
        in_test: false,
        out: Vec::new(),
    };
    scan.visit_file(&parsed);
    Some(scan.out)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && !vendored(p))
        .collect();
    if files.is_empty() {
        return Verdict::NotApplicable("the repository tracks no Rust source file".into());
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    for path in &files {
        if !view.read(path).is_some_and(|t| t.contains("current_exe")) {
            continue;
        }
        let Some(found) = view.read(path).and_then(|t| sites(path, &t)) else {
            unparsed.push(path);
            continue;
        };
        for site in found {
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!(
                        "test `{}` finds a built artifact by walking up from `current_exe()`",
                        site.function
                    ),
                    format!(
                        "`{}` in `{path}` takes `std::env::current_exe()` and goes up or joins a directory it assumes ({}), which hard-codes where Cargo \
                         puts build products and breaks when Cargo changes its layout. Ask Cargo instead: `env!(\"CARGO_BIN_EXE_<name>\")` is the absolute \
                         path of a binary of the same package (integration tests and benchmarks), `CARGO_MANIFEST_DIR` and `CARGO_TARGET_TMPDIR` locate the \
                         package and a scratch directory, and `OUT_DIR` a build script's output. An example has no such variable: make the helper a `[[bin]]` \
                         target, or build it through cargo and read its path from cargo's JSON output. Read from the syntax only: a helper in another \
                         file that does the walk is not seen.",
                        site.function, site.how
                    ),
                )
                .location(Location::new(path.as_str(), Some(site.line)))
                .subject(format!("{path}:{}", site.function))
                .positive_control(format!(
                    "{} Rust file(s) read; the function was read at {path}:{}",
                    files.len() - unparsed.len(),
                    site.line
                )),
            );
        }
    }
    let mut notes = vec![format!(
        "{ID}: {} Rust file(s) read with syn",
        files.len() - unparsed.len()
    )];
    if !unparsed.is_empty() {
        notes.push(format!(
            "{ID}: {} file(s) could not be parsed and were not read: {}",
            unparsed.len(),
            unparsed.join(", ")
        ));
    }
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn got(path: &str, src: &str) -> Vec<(u32, String)> {
        sites(path, src)
            .expect("parses")
            .into_iter()
            .map(|s| (s.line, s.function))
            .collect()
    }

    #[test]
    fn a_walk_up_from_current_exe_to_a_sibling_directory_is_reported() {
        let src = "fn helper() -> std::path::PathBuf {\n    let exe = std::env::current_exe().expect(\"x\");\n    let debug = exe.parent().and_then(Path::parent).expect(\"t\");\n    debug.join(\"examples\").join(\"tool\")\n}";
        assert_eq!(got("tests/e2e.rs", src), vec![(3, "helper".to_string())]);
    }

    #[test]
    fn pop_on_a_local_from_current_exe_is_reported_and_one_finding_per_function() {
        let src = "fn bin() {\n    let mut p = std::env::current_exe().unwrap();\n    p.pop();\n    p.pop();\n    p.push(\"examples\");\n}";
        assert_eq!(got("tests/ci.rs", src), vec![(3, "bin".to_string())]);
    }

    #[test]
    fn a_chain_straight_off_the_call_is_reported() {
        let src =
            "fn f() { let _ = std::env::current_exe().unwrap().parent().unwrap().join(\"deps\"); }";
        assert_eq!(got("examples/x.rs", src), vec![(1, "f".to_string())]);
    }

    #[test]
    fn running_the_test_binary_itself_is_not_a_walk() {
        let src = "fn child() { let _ = Command::new(std::env::current_exe().unwrap()).args([\"--exact\", \"t\"]).spawn(); }\nfn print() { println!(\"{:?}\", std::env::current_exe()); }";
        assert!(got("tests/e2e.rs", src).is_empty());
    }

    #[test]
    fn non_test_code_is_not_read_but_a_cfg_test_module_in_it_is() {
        let walk = "fn sibling() { let me = std::env::current_exe().unwrap(); let _ = me.with_file_name(\"other\"); }";
        assert!(got("src/launcher.rs", walk).is_empty());
        let in_test = format!("#[cfg(test)]\nmod tests {{\n    {walk}\n}}");
        assert_eq!(
            got("src/launcher.rs", &in_test),
            vec![(3, "sibling".to_string())]
        );
        let attr = format!("#[test]\n{walk}");
        assert_eq!(
            got("src/launcher.rs", &attr),
            vec![(2, "sibling".to_string())]
        );
    }

    #[test]
    fn a_function_that_reads_cargos_own_variable_is_exempt() {
        let src = "fn bin() -> String {\n    let _ = std::env::current_exe().unwrap().parent();\n    env!(\"CARGO_BIN_EXE_tool\").to_string()\n}\nfn other() { let _ = std::env::current_exe().unwrap().parent(); }";
        assert_eq!(got("tests/e2e.rs", src), vec![(5, "other".to_string())]);
    }

    #[test]
    fn joining_a_directory_name_onto_a_path_that_is_not_from_current_exe_is_not_reported() {
        let src = "fn f(root: &Path) { let _ = root.join(\"examples\"); let _ = root.parent(); }";
        assert!(got("tests/e2e.rs", src).is_empty());
    }

    #[test]
    fn a_file_syn_cannot_parse_is_none() {
        assert!(sites("tests/x.rs", "fn (((").is_none());
    }
}
