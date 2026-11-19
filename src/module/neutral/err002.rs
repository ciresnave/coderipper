// SPDX-License-Identifier: MIT OR Apache-2.0
//! ERR-002: a catch-all handler sits only at a boundary.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. The one handler Rust has that catches every
//! kind of failure is the panic catcher: a call of `catch_unwind` (`std::panic::catch_unwind`, `panic::catch_unwind`, a bare
//! `catch_unwind`, whether or not the file imports it: any call whose path ends in `catch_unwind`) or a `.catch_unwind()` method (`futures::FutureExt`). It is reported when it is in a function that is
//! not a *boundary*: a boundary is `main`, any function of a binary target (`src/main.rs`, `src/bin/..`), and a function whose name says
//! it is an outermost one (one of its `_`-separated words is `run`, `serve`, `handle`, `handler`, `dispatch`, `worker`, `listener`, `supervise`,
//! `supervisor`, `boundary`, `entry`, `entrypoint`, `extern`, `ffi`, `callback` or `trampoline`: `run_server` is one, `truncate` is not). A `//` comment on the line or in the three lines above that says why
//! (`boundary`, `outermost`, `top-level`, `top level`, `ffi`, `isolate`, `supervis`, `must not unwind`, `across the`) also clears it.
//! One **Medium**-severity, Medium-confidence finding per call, at the call.
//!
//! **What it cannot see:** a catch-all that is not a panic catcher (a `match` arm `Err(_) => ..` that swallows is `ERR-001`'s), a
//! boundary the project declares elsewhere, and whether the function is in fact the outermost one of its task. A file `syn` cannot
//! parse is named in a note, never read as clean. Exception in 0.47.12: when it is the repository's only Rust file the run says `not applicable` and does not name it (fixed in 0.47.14).

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::ERR_002;

const BOUNDARY_NAMES: &[&str] = &[
    "main",
    "run",
    "serve",
    "handle",
    "handler",
    "dispatch",
    "worker",
    "listener",
    "supervise",
    "supervisor",
    "entrypoint",
    "boundary",
    "entry",
    "ffi",
    "callback",
    "trampoline",
];

const JUSTIFICATIONS: &[&str] = &[
    "boundary",
    "outermost",
    "top-level",
    "top level",
    "ffi",
    "isolate",
    "supervis",
    "must not unwind",
    "across the",
];

/// A boundary by name: one of the `_`-separated words of the name is a boundary word (`run_server`, `handle_request`, `extern_entry`),
/// so `truncate` is not read as `run`.
fn boundary_name(name: &str) -> bool {
    let n = name.to_lowercase();
    n.split('_')
        .any(|w| BOUNDARY_NAMES.contains(&w) || w == "extern")
}

fn is_binary_path(path: &str) -> bool {
    path == "src/main.rs"
        || path.ends_with("/src/main.rs")
        || path.contains("/src/bin/")
        || path.starts_with("src/bin/")
}

struct Site {
    line: u32,
    function: String,
}

struct Walk<'a> {
    lines: Vec<&'a str>,
    binary: bool,
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

impl Walk<'_> {
    fn justified(&self, line: u32) -> bool {
        let says = |l: &str| {
            l.find("//").is_some_and(|i| {
                let comment = l[i..].to_lowercase();
                JUSTIFICATIONS.iter().any(|j| comment.contains(j))
            })
        };
        let line = usize::try_from(line).unwrap_or(0);
        if self
            .lines
            .get(line.wrapping_sub(1))
            .is_some_and(|l| says(l))
        {
            return true;
        }
        for n in (line.saturating_sub(3).max(1)..line).rev() {
            let Some(l) = self.lines.get(n - 1) else {
                break;
            };
            let t = l.trim_start();
            if t.is_empty() {
                continue;
            }
            if !t.starts_with("//") {
                break;
            }
            if says(l) {
                return true;
            }
        }
        false
    }

    fn catch(&mut self, line: u32) {
        let function = self
            .functions
            .last()
            .cloned()
            .unwrap_or_else(|| "<module>".into());
        let outermost = self.binary || self.functions.iter().any(|f| boundary_name(f));
        if outermost || self.justified(line) {
            return;
        }
        self.sites.push(Site { line, function });
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
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

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            if p.path
                .segments
                .last()
                .is_some_and(|s| s.ident == "catch_unwind")
            {
                self.catch(u32::try_from(c.func.span().start().line).unwrap_or(0));
            }
        }
        syn::visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        if m.method == "catch_unwind" {
            self.catch(u32::try_from(m.method.span().start().line).unwrap_or(0));
        }
        syn::visit::visit_expr_method_call(self, m);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return super::rust_package::nothing_to_read(ID, &loaded);
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            let mut walk = Walk {
                lines: s.text.lines().collect(),
                binary: is_binary_path(&s.path),
                functions: Vec::new(),
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        view.name(),
                        format!(
                            "`catch_unwind` in `{}` catches every panic away from a boundary",
                            site.function
                        ),
                        "A catch-all deep inside the code turns distinct bugs into one outcome and hides causes that should surface. Let the panic propagate to the outermost function of the task (a request loop, a task runner, an FFI entry), catch it there, or say in a comment on the call why this is that boundary.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!("{}:{}", s.path, site.function))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads panic catchers and the functions around them"
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

    fn found_in(path: &str, src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            (path, src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn found(src: &str) -> Vec<Finding> {
        found_in("src/lib.rs", src)
    }

    #[test]
    fn a_panic_catcher_in_an_ordinary_function_is_reported() {
        let src = "pub fn parse(s: &str) -> Option<u8> {
    std::panic::catch_unwind(|| s.parse().unwrap()).ok()
}
pub async fn fetch() { let _ = work().catch_unwind().await; }
";
        let f = found(src);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(f[0].subject.as_deref().unwrap().ends_with(":parse"));
    }

    #[test]
    fn a_boundary_by_name_by_target_or_by_comment_is_not_reported() {
        let named = "pub fn run_server() { let _ = std::panic::catch_unwind(|| serve()); }\nfn main() { let _ = std::panic::catch_unwind(real_main); }\nextern \"C\" fn extern_entry() { let _ = std::panic::catch_unwind(|| 1); }\n";
        assert!(found(named).is_empty(), "{:?}", found(named));
        let not_a_word = "pub fn truncate() { let _ = std::panic::catch_unwind(|| 1); }
";
        assert_eq!(
            found(not_a_word).len(),
            1,
            "`truncate` holds `run` but is not a boundary"
        );
        let binary = "fn helper() { let _ = std::panic::catch_unwind(|| 1); }\n";
        assert!(found_in("src/main.rs", binary).is_empty());
        assert_eq!(
            found(binary).len(),
            1,
            "the same code in a library is reported"
        );
        let commented = "pub fn f() {\n    // the FFI boundary: a panic must not unwind into C\n    let _ = std::panic::catch_unwind(|| 1);\n}\n";
        assert!(found(commented).is_empty(), "{:?}", found(commented));
    }

    #[test]
    fn test_code_is_not_read() {
        let src =
            "#[cfg(test)]\nmod t { pub fn f() { let _ = std::panic::catch_unwind(|| 1); } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        for f in found("pub fn f() { let _ = std::panic::catch_unwind(|| 1); }\n") {
            f.validate().unwrap();
        }
    }
}

/// Claims audit (2026-11-17): one test per claim of the module documentation, the catalog `checked_today` and the changelog entry.
#[cfg(test)]
mod claims {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found_in(path: &str, src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            (path, src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    fn found(src: &str) -> Vec<Finding> {
        found_in("src/lib.rs", src)
    }

    #[test]
    fn every_documented_call_form_is_a_panic_catcher() {
        for call in [
            "std::panic::catch_unwind(|| 1)",
            "panic::catch_unwind(|| 1)",
            "catch_unwind(|| 1)",
            "fut.catch_unwind()",
        ] {
            let src = format!("pub fn parse() {{ let _ = {call}; }}");
            assert_eq!(found(&src).len(), 1, "{call}");
        }
    }

    #[test]
    fn every_boundary_word_makes_a_function_a_boundary_and_a_word_inside_another_word_does_not() {
        for word in [
            "main",
            "run",
            "serve",
            "handle",
            "handler",
            "dispatch",
            "worker",
            "listener",
            "supervise",
            "supervisor",
            "boundary",
            "entry",
            "entrypoint",
            "extern",
            "ffi",
            "callback",
            "trampoline",
        ] {
            let src =
                format!("pub fn do_{word}_now() {{ let _ = std::panic::catch_unwind(|| 1); }}");
            assert!(found(&src).is_empty(), "{word}: {:?}", found(&src));
        }
        for name in ["truncate", "prune", "reentry", "mainly"] {
            let src = format!("pub fn {name}() {{ let _ = std::panic::catch_unwind(|| 1); }}");
            assert_eq!(found(&src).len(), 1, "{name} is not a boundary word");
        }
    }

    #[test]
    fn a_method_of_a_boundary_named_function_and_a_nested_call_in_a_boundary_are_boundaries() {
        let method = "pub struct S;\nimpl S { pub fn handle_request(&self) { let _ = std::panic::catch_unwind(|| 1); } }\n";
        assert!(found(method).is_empty(), "{:?}", found(method));
    }

    #[test]
    fn binary_targets_are_boundaries_by_path() {
        let src = "fn helper() { let _ = std::panic::catch_unwind(|| 1); }\n";
        assert!(found_in("src/main.rs", src).is_empty());
        assert!(found_in("src/bin/tool.rs", src).is_empty());
        assert_eq!(
            found_in("src/util.rs", src).len(),
            1,
            "positive control: an ordinary module is read"
        );
    }

    #[test]
    fn a_justifying_comment_counts_on_the_line_and_up_to_three_lines_above_and_not_four() {
        let same =
            "pub fn f() { let _ = std::panic::catch_unwind(|| 1); // the outermost function\n}\n";
        assert!(found(same).is_empty(), "{:?}", found(same));
        let three = "pub fn f() {\n    // isolate the plugin\n    //\n    //\n    let _ = std::panic::catch_unwind(|| 1);\n}\n";
        assert!(found(three).is_empty(), "{:?}", found(three));
        let four = "pub fn f() {\n    // isolate the plugin\n    let a = 1;\n    let b = 2;\n    let _ = std::panic::catch_unwind(|| a + b);\n}\n";
        assert_eq!(
            found(four).len(),
            1,
            "a comment above other code is not this call's"
        );
        for word in [
            "boundary",
            "outermost",
            "top-level",
            "top level",
            "ffi",
            "isolate",
            "supervis",
            "must not unwind",
            "across the",
        ] {
            let src = format!(
                "pub fn f() {{\n    // {word}\n    let _ = std::panic::catch_unwind(|| 1);\n}}\n"
            );
            assert!(found(&src).is_empty(), "{word}");
        }
    }

    #[test]
    fn a_swallowing_err_arm_is_not_this_rules_and_the_finding_is_medium_medium() {
        let swallow = "pub fn f() { match g() { Ok(_) => {}, Err(_) => {} } }\n";
        assert!(found(swallow).is_empty(), "{:?}", found(swallow));
        let f = found("pub fn f() { let _ = std::panic::catch_unwind(|| 1); }\n");
        assert_eq!(
            (f[0].severity, f[0].confidence),
            (Severity::Medium, Confidence::Medium)
        );
    }
}
