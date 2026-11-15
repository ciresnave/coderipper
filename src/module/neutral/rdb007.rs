// SPDX-License-Identifier: MIT OR Apache-2.0
//! RDB-007: a bare literal whose meaning is not obvious gets a name.
//!
//! **What this judges (Rust, from the source alone):** every tracked production `.rs` file (outside vendored directories, tests,
//! examples, tools, `build.rs`, files over 2 MiB, and outside `#[cfg(test)]` and `#[test]` items) is parsed with `syn`. Two shapes are
//! counted per file and reported as **one Info finding per file at low confidence**, at the first site, with the counts and the first
//! sites:
//!
//! 1. **Numbers:** an integer or float literal (decimal, hex, octal or binary) that is an operand of an arithmetic, comparison or
//!    compound-assignment operator, an argument of a function or method call, or a bare value (a `let` initialiser, a `return`
//!    value, the right side of `=`, a struct-field value, a block's tail expression), and is not in the trivial set: `0`, `1`, `2`, `10`, `100`, `1000` (`1_000`), `1024`, `255`, `60`,
//!    `24`, the powers of two up to `4096`, and the floats `0.0`, `1.0` and `0.5`; a negated trivial value is trivial. Not read: a
//!    `const` or `static` item (the name *is* the fix), an enum discriminant, an array length (type or repeat), an attribute, a macro
//!    argument, and the arguments of a tuple-struct or enum constructor (a callee whose last segment starts with a capital).
//! 2. **Strings:** a string literal of at least two characters that appears three or more times in the file with the same text
//!    (outside `const` and `static` items, attributes and macro arguments, so a format string is never counted).
//!
//! The figures are chosen to report only the plain cases; a literal is a cue to read, never a verdict, and the subject is the file
//! path (an allowlist entry for it silences both).
//!
//! **What it cannot see:** a literal inside a macro (`vec![86400]`, `assert_eq!`, `format!`), a literal copied through a variable, a
//! number written as a constant expression (`60 * 60 * 24` is read as three trivial operands except where one is not in the set), a
//! string repeated across files, and whether a name would help. A file `syn` cannot parse is named in a note, never read as clean.

use super::rust_src::{is_test_item, line_of, production_files, MAX_BYTES};
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::visit::Visit;

const ID: &str = crate::rule_id::RDB_007;

/// A string must be repeated this many times in a file to be reported.
const MIN_REPEATS: usize = 3;

/// A string must be at least this many characters to be counted.
const MIN_CHARS: usize = 2;

/// How many first sites the finding names, per shape.
const SHOWN: usize = 3;

const TRIVIAL_INTS: &[u128] = &[0, 1, 2, 10, 100, 1000, 1024, 255, 60, 24];

const TRIVIAL_FLOATS: &[f64] = &[0.0, 1.0, 0.5];

fn trivial_int(v: u128) -> bool {
    TRIVIAL_INTS.contains(&v) || (v.is_power_of_two() && v <= 4096)
}

/// The text of a numeric literal operand (a leading `-` and parentheses looked through) when it is not trivial.
fn magic(e: &syn::Expr) -> Option<String> {
    match e {
        syn::Expr::Paren(p) => magic(&p.expr),
        syn::Expr::Unary(u) if matches!(u.op, syn::UnOp::Neg(_)) => {
            magic(&u.expr).map(|t| format!("-{t}"))
        }
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => {
                let v = i.base10_parse::<u128>().ok()?;
                (!trivial_int(v)).then(|| i.base10_digits().to_string())
            }
            syn::Lit::Float(f) => {
                let v = f.base10_parse::<f64>().ok()?;
                (!TRIVIAL_FLOATS.iter().any(|t| (t - v).abs() < f64::EPSILON))
                    .then(|| f.base10_digits().to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

#[derive(Default)]
struct Scan {
    numbers: Vec<(u32, String)>,
    strings: BTreeMap<String, Vec<u32>>,
}

impl Scan {
    fn operand(&mut self, e: &syn::Expr) {
        if let Some(text) = magic(e) {
            self.numbers
                .push((line_of(syn::spanned::Spanned::span(e)), text));
        }
    }
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_fn(self, i);
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_item_impl(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_impl_item_fn(self, i);
        }
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if !is_test_item(&i.attrs) {
            syn::visit::visit_trait_item_fn(self, i);
        }
    }

    // a name is the fix, so what a constant holds is not read
    fn visit_item_const(&mut self, _: &'ast syn::ItemConst) {}
    fn visit_item_static(&mut self, _: &'ast syn::ItemStatic) {}
    fn visit_impl_item_const(&mut self, _: &'ast syn::ImplItemConst) {}
    fn visit_trait_item_const(&mut self, _: &'ast syn::TraitItemConst) {}
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
    // a discriminant is a number the enum names
    fn visit_variant(&mut self, _: &'ast syn::Variant) {}

    fn visit_type_array(&mut self, t: &'ast syn::TypeArray) {
        self.visit_type(&t.elem);
    }

    fn visit_expr_repeat(&mut self, e: &'ast syn::ExprRepeat) {
        self.visit_expr(&e.expr);
    }

    // a bare value: `let t = 86400;`, `return 86400;`, `x = 86400`, `S { t: 86400 }` and a block's tail
    fn visit_local(&mut self, l: &'ast syn::Local) {
        if let Some(init) = &l.init {
            self.operand(&init.expr);
        }
        syn::visit::visit_local(self, l);
    }

    fn visit_expr_return(&mut self, e: &'ast syn::ExprReturn) {
        if let Some(v) = &e.expr {
            self.operand(v);
        }
        syn::visit::visit_expr_return(self, e);
    }

    fn visit_expr_assign(&mut self, e: &'ast syn::ExprAssign) {
        self.operand(&e.right);
        syn::visit::visit_expr_assign(self, e);
    }

    fn visit_expr_struct(&mut self, e: &'ast syn::ExprStruct) {
        for f in &e.fields {
            self.operand(&f.expr);
        }
        syn::visit::visit_expr_struct(self, e);
    }

    fn visit_block(&mut self, b: &'ast syn::Block) {
        if let Some(syn::Stmt::Expr(tail, None)) = b.stmts.last() {
            self.operand(tail);
        }
        syn::visit::visit_block(self, b);
    }

    fn visit_expr_binary(&mut self, e: &'ast syn::ExprBinary) {
        self.operand(&e.left);
        self.operand(&e.right);
        syn::visit::visit_expr_binary(self, e);
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        let constructor = matches!(&*e.func, syn::Expr::Path(p)
            if p.path.segments.last().is_some_and(|s| s.ident.to_string().starts_with(char::is_uppercase)));
        if !constructor {
            for a in &e.args {
                self.operand(a);
            }
        }
        syn::visit::visit_expr_call(self, e);
    }

    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        for a in &e.args {
            self.operand(a);
        }
        syn::visit::visit_expr_method_call(self, e);
    }

    fn visit_lit_str(&mut self, s: &'ast syn::LitStr) {
        let text = s.value();
        if text.chars().count() >= MIN_CHARS {
            self.strings
                .entry(text)
                .or_default()
                .push(line_of(s.span()));
        }
    }
}

struct Hit {
    line: u32,
    summary: String,
}

fn shorten(s: &str) -> String {
    let one: String = s.chars().take(30).collect();
    if s.chars().count() > 30 {
        format!("{one}...")
    } else {
        one
    }
}

fn read(path: &str, scan: Scan) -> Option<Hit> {
    let mut numbers = scan.numbers;
    numbers.sort();
    let mut repeated: Vec<(u32, String, usize)> = scan
        .strings
        .into_iter()
        .filter(|(_, lines)| lines.len() >= MIN_REPEATS)
        .map(|(text, lines)| (*lines.iter().min().unwrap_or(&0), text, lines.len()))
        .collect();
    repeated.sort();
    if numbers.is_empty() && repeated.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut first = u32::MAX;
    if !numbers.is_empty() {
        first = first.min(numbers[0].0);
        let sites: Vec<String> = numbers
            .iter()
            .take(SHOWN)
            .map(|(l, t)| format!("{t} at line {l}"))
            .collect();
        parts.push(format!(
            "{} number(s) with no name (first: {})",
            numbers.len(),
            sites.join(", ")
        ));
    }
    if !repeated.is_empty() {
        first = first.min(repeated[0].0);
        let sites: Vec<String> = repeated
            .iter()
            .take(SHOWN)
            .map(|(l, t, n)| format!("\"{}\" x{n} from line {l}", shorten(t)))
            .collect();
        parts.push(format!(
            "{} string(s) repeated {MIN_REPEATS} or more times (first: {})",
            repeated.len(),
            sites.join(", ")
        ));
    }
    Some(Hit {
        line: first,
        summary: format!("{path}: {}", parts.join("; ")),
    })
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = production_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust source outside tests, tooling and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    let mut unparsed: Vec<&str> = Vec::new();
    let mut oversize: Vec<&str> = Vec::new();
    let mut parsed = 0usize;
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
        let mut scan = Scan::default();
        scan.visit_file(&file);
        if let Some(hit) = read(path, scan) {
            findings.push((path.clone(), hit));
        }
    }
    findings.sort_by(|a, b| a.0.cmp(&b.0));
    let total = files.len();
    let out: Vec<Finding> = findings
        .into_iter()
        .map(|(path, h)| {
            Finding::new(
                ID,
                Severity::Info,
                Confidence::Low,
                view.name(),
                h.summary,
                "Introduce a named constant or a small type that states the meaning, and use it at each site. A value that is obvious where it stands needs no name: allowlist the file with the reason.",
            )
            .location(Location::new(path.as_str(), Some(h.line)))
            .subject(path.clone())
            .positive_control(format!(
                "{total} production Rust file(s) tracked, {parsed} of them parsed and walked for literals; this file is {path}"
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
    fn a_number_in_a_comparison_arithmetic_or_call_is_reported() {
        let f = found("fn f(x: u64) -> u64 {\n    if x > 86400 { return 7; }\n    g(x * 37)\n}\nfn g(_: u64) -> u64 { 0 }\n");
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
        assert!(f[0].summary.contains("3 number(s)"), "{}", f[0].summary);
        assert_eq!(f[0].subject.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn the_trivial_set_and_negated_trivial_values_are_not_reported() {
        let src = "fn f(x: i64, y: f64) -> f64 {\n    let _ = (x + 1, x - 2, x * 10, x / 100, x % 1000, x > 1024, x == 255, x < 60, x > 24, x << 4096, x == -1, x > 0);\n    y * 0.5 + 1.0 + 0.0\n}\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn constants_discriminants_array_lengths_attributes_and_tests_are_not_read() {
        let src = "const LIMIT: u64 = 86400;\nstatic NAME: &str = \"abc\";\nenum E { A = 7, B = 9 }\nstruct S { a: [u8; 37] }\n#[allow(clippy::too_many_lines = 37)]\nfn f() -> [u8; 91] { [0; 91] }\n#[cfg(test)]\nmod t { fn g(x: u64) -> bool { x > 86400 } }\n#[test]\nfn h() { assert!(1 + 86400 > 3); }\nmacro_rules! m { () => { 7 + 8 } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_constructor_argument_and_a_macro_argument_are_not_read() {
        let src = "struct W(u64);\nfn f() -> W { println!(\"{}\", 86400); W(86400) }\nfn g() -> Option<u64> { Some(86400) }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_string_repeated_three_times_is_reported_and_two_are_not() {
        let src = "fn f() { a(\"connection refused\"); b(\"connection refused\"); c(\"connection refused\"); a(\"twice\"); b(\"twice\"); a(\"\"); a(\"\"); a(\"\"); a(\"x\"); a(\"x\"); a(\"x\"); }\nfn a(_: &str) {}\nfn b(_: &str) {}\nfn c(_: &str) {}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("1 string(s)"), "{}", f[0].summary);
        assert!(
            f[0].summary.contains("connection refused"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn numbers_and_strings_in_one_file_are_one_finding() {
        let src = "fn f(x: u64) { if x > 777 {} a(\"same text\"); a(\"same text\"); a(\"same text\"); }\nfn a(_: &str) {}\n";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("number(s)") && f[0].summary.contains("string(s)"));
    }

    #[test]
    fn compound_assignment_is_read() {
        let f = found(
            "fn f(mut x: u64) { x += 86400; }
",
        );
        assert_eq!(f.len(), 1, "{f:?}");
    }

    #[test]
    fn a_bare_value_in_a_binding_return_field_assignment_or_tail_is_reported() {
        for src in [
            "fn f() -> u64 { let timeout = 86400; timeout }
",
            "fn f() -> u64 { return 86400; }
",
            "fn f() -> u64 { 86400 }
",
            "struct S { t: u64 }
fn f() -> S { S { t: 86400 } }
",
            "fn f(x: &mut u64) { *x = 86400; }
",
        ] {
            assert_eq!(found(src).len(), 1, "{src}");
        }
        let trivial = "fn f() -> u64 { let a = 1; let b = 0; if a > b { return 60; } 1024 }
";
        assert!(found(trivial).is_empty(), "{:?}", found(trivial));
    }

    #[test]
    fn hex_octal_and_binary_literals_are_read() {
        for src in [
            "fn f(x: u64) -> bool { x > 0xDEAD_BEEF }
",
            "fn f(x: u64) -> bool { x > 0o7777 }
",
            "fn f(x: u64) -> bool { x > 0b1010_1010_1 }
",
        ] {
            assert_eq!(found(src).len(), 1, "{src}");
        }
        assert!(found(
            "fn f(x: u64) -> bool { x > 0x40 && x < 0b1000 }
"
        )
        .is_empty());
    }

    #[test]
    fn a_file_over_the_size_cap_is_named_in_a_note() {
        let big = format!(
            "fn f(x: u64) -> bool {{ x > 86400 }}
// {}
",
            "a".repeat(MAX_BYTES)
        );
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", &big),
        ])) {
            Verdict::Noted(f, notes) => {
                assert!(f.is_empty());
                assert!(
                    notes[0].contains("src/lib.rs") && notes[0].contains("2 MiB"),
                    "{notes:?}"
                );
            }
            _ => panic!("expected a note"),
        }
    }

    #[test]
    fn every_finding_validates() {
        for f in found("fn f(x: u64) -> bool { x > 86400 }\n") {
            f.validate().unwrap();
        }
    }
}
