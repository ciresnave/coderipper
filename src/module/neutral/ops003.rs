// SPDX-License-Identifier: MIT OR Apache-2.0
//! OPS-003: a library writes no raw output (`println!`, `print!`, `eprintln!`, `eprint!`, `dbg!`); it uses a logging facility.
//!
//! **What this judges (Rust, from the source alone):** only a package that is a library (it has `src/lib.rs`, or its `Cargo.toml` has a
//! `[lib]` table). Every tracked `.rs` file of that package is parsed with `syn`, outside vendored directories, tests, examples, tools and
//! `build.rs` (the shared `rust_package::load` walk), outside a binary's source (`main.rs`, `src/bin/`), and outside `#[cfg(test)]`
//! modules and test functions. Each call of one of the five macros is a finding, at the macro call.
//!
//! **What it cannot see:** a print made by a macro the package defines or by a dependency's macro that expands to one, a write to
//! `std::io::stdout()` or `stderr()` by hand, a print inside another macro's arguments (`syn` does not look into them), another
//! language, and whether the library is a command-line tool's own support code, which prints by design (that is a waiver with the
//! reason). A file `syn` cannot parse is named in a note, never read as clean. Low severity, High confidence: the call is exactly the
//! thing the rule names.

use super::rust_package::{load, unparsed_note, Package};
use super::{join, RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::OPS_003;

const PRINT_MACROS: &[&str] = &["println", "print", "eprintln", "eprint", "dbg"];

fn is_binary_source(path: &str) -> bool {
    path == "main.rs"
        || path.ends_with("/main.rs")
        || path.starts_with("src/bin/")
        || path.contains("/src/bin/")
}

fn is_library(view: &RepoView, p: &Package) -> bool {
    view.has(&join(&p.dir, "src/lib.rs"))
        || view
            .read(&join(&p.dir, "Cargo.toml"))
            .is_some_and(|m| m.lines().any(|l| l.trim() == "[lib]"))
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

struct Site {
    line: u32,
    function: String,
    name: String,
}

#[derive(Default)]
struct Walk {
    functions: Vec<String>,
    sites: Vec<Site>,
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if let Some(last) = m.path.segments.last() {
            let name = last.ident.to_string();
            if PRINT_MACROS.contains(&name.as_str()) {
                self.sites.push(Site {
                    line: u32::try_from(m.path.span().start().line).unwrap_or(0),
                    function: self
                        .functions
                        .last()
                        .cloned()
                        .unwrap_or_else(|| "<module>".into()),
                    name,
                });
            }
        }
        syn::visit::visit_macro(self, m);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    let libraries: Vec<&Package> = loaded
        .packages
        .iter()
        .filter(|p| is_library(view, p))
        .collect();
    if libraries.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust library package (a package with src/lib.rs or a [lib] table) outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in libraries {
        let files: Vec<_> = p
            .sources
            .iter()
            .filter(|s| !is_binary_source(&s.path))
            .collect();
        let count = files.len();
        for s in files {
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Low,
                        Confidence::High,
                        view.name(),
                        format!("in `{}`: `{}!` in library code", site.function, site.name),
                        "A library that writes to the console takes that decision away from the application that embeds it, and a raw print cannot be filtered, routed or correlated. Emit a record through the project's logger (`log` or `tracing`) at a suitable level, or return the text to the caller. A library that exists to print (a command-line tool's support code) is a waiver with the reason.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}:{}", s.path, site.function, site.name))
                    .positive_control(format!(
                        "{count} non-binary Rust source file(s) of a library package were parsed with the same walk that reads the print macros"
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn each_print_macro_in_a_library_is_reported() {
        let src = "pub fn a() { println!(\"x\"); print!(\"x\"); eprintln!(\"x\"); eprint!(\"x\"); let _ = dbg!(1); std::println!(\"y\"); }\n";
        let f = run(&[("Cargo.toml", MANIFEST), ("src/lib.rs", src)]);
        assert_eq!(f.len(), 6, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(1));
    }

    #[test]
    fn tests_binaries_and_logging_are_not_reported() {
        let lib = "pub fn a() { log::info!(\"x\"); let _ = format!(\"x\"); }
#[cfg(test)] mod t { fn p() { println!(\"x\"); } }
#[test] fn u() { dbg!(1); }
";
        let f = run(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", lib),
            ("src/main.rs", "fn main() { println!(\"hi\"); }"),
            ("src/bin/tool.rs", "fn main() { eprintln!(\"hi\"); }"),
        ]);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn a_binary_only_package_is_not_applicable() {
        let v = judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/main.rs", "fn main() { println!(\"hi\"); }"),
        ]));
        assert!(matches!(v, Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_lib_table_makes_a_library() {
        let m = format!("{MANIFEST}[lib]\npath = \"src/core.rs\"\n");
        let f = run(&[
            ("Cargo.toml", m.as_str()),
            ("src/core.rs", "pub fn a() { println!(\"x\"); }"),
        ]);
        assert_eq!(f.len(), 1, "{f:?}");
    }
}
