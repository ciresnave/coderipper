// SPDX-License-Identifier: MIT OR Apache-2.0
//! TST-009: a Rust package that takes untrusted input or holds a lot of `unsafe` has a property or fuzz target.
//!
//! **What this judges (Rust, from the source and the manifest alone):** for each Cargo package (the nearest tracked `Cargo.toml` above
//! a source file; files in vendored directories, tests, examples, tools, `build.rs` and `#[cfg(test)]` items are not read) it decides
//! whether the package is *risky*, and if it is, whether it has a generated-input target.
//!
//! * **Risky, by input:** the package has a `pub fn` (free, or an inherent method) whose name starts with `parse`, `decode`,
//!   `deserialize`, `from_str` or `from_bytes` and that takes text or bytes (`&str`, `String`, `&[u8]`, `Vec<u8>`, `Bytes`), or an
//!   `impl FromStr for` a type.
//! * **Risky, by `unsafe`:** the package has at least **five** `unsafe` blocks and `unsafe fn`s together.
//! * **Has a target:** the package's `Cargo.toml`, or the repository root one, names a property-testing or fuzzing crate (`proptest`,
//!   `quickcheck`, `arbitrary`, `bolero`, `arbtest`, `libfuzzer-sys`, `honggfuzz`, `afl`), or the package or the repository root has a
//!   tracked `fuzz/Cargo.toml` (the `cargo fuzz` layout).
//!
//! A risky package with no target gets one **Medium**-severity, Low-confidence finding, at the package's `Cargo.toml`, that says
//! which of the two reasons applied and names up to three of the functions.
//!
//! **What it cannot see (and the record says so):** that a target is *wired into the pipeline* (the catalog asks for it to be run
//! regularly; this reads that one exists); whether a target covers the risky module rather than another one; input that is not
//! named `parse`/`decode`/...; a hand-rolled generated-input test with no crate behind it; modules rather than packages (the unit
//! here is the package, the module level needs a classification this does not have); other languages. A file `syn` cannot parse is
//! named in a note, never read as clean.

use super::rust_package::{load, unparsed_note, Package};
use super::{join, RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::visit::Visit;

const ID: &str = crate::rule_id::TST_009;

/// `unsafe` sites at which a package counts as unsafe-heavy.
const UNSAFE_SITES: usize = 5;

/// Name prefixes of functions that take input to be decoded.
const INPUT_PREFIXES: &[&str] = &["parse", "decode", "deserialize", "from_str", "from_bytes"];

/// Crates that generate test input.
const TARGET_CRATES: &[&str] = &[
    "proptest",
    "quickcheck",
    "arbitrary",
    "bolero",
    "arbtest",
    "libfuzzer-sys",
    "honggfuzz",
    "afl",
];

fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

/// Whether a type is text or bytes: `&str`, `String`, `&[u8]`, `Vec<u8>`, `Bytes`.
fn is_text_or_bytes(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(r) => is_text_or_bytes(&r.elem),
        syn::Type::Slice(s) => matches!(&*s.elem, syn::Type::Path(p) if p.path.is_ident("u8")),
        syn::Type::Path(p) => p.path.segments.last().is_some_and(|s| {
            let n = s.ident.to_string();
            n == "str" || n == "String" || n == "Bytes" || n == "Vec" && vec_of_u8(&s.arguments)
        }),
        _ => false,
    }
}

fn vec_of_u8(args: &syn::PathArguments) -> bool {
    let syn::PathArguments::AngleBracketed(a) = args else {
        return false;
    };
    a.args.iter().any(
        |g| matches!(g, syn::GenericArgument::Type(syn::Type::Path(p)) if p.path.is_ident("u8")),
    )
}

fn takes_input(sig: &syn::Signature) -> bool {
    let name = sig.ident.to_string();
    INPUT_PREFIXES.iter().any(|p| name.starts_with(p))
        && sig.inputs.iter().any(|a| match a {
            syn::FnArg::Typed(t) => is_text_or_bytes(&t.ty),
            syn::FnArg::Receiver(_) => false,
        })
}

#[derive(Default)]
struct Risk {
    input_fns: Vec<String>,
    from_str_impls: usize,
    unsafe_sites: usize,
}

impl<'ast> Visit<'ast> for Risk {
    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        self.unsafe_sites += 1;
        syn::visit::visit_expr_unsafe(self, e);
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_cfg_test(&i.attrs) {
            syn::visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if matches!(i.sig.safety, syn::Safety::Unsafe(_)) {
            self.unsafe_sites += 1;
        }
        if is_pub(&i.vis) && takes_input(&i.sig) {
            self.input_fns.push(i.sig.ident.to_string());
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        match &i.trait_ {
            Some((path, _)) => {
                if path.segments.last().is_some_and(|s| s.ident == "FromStr") {
                    self.from_str_impls += 1;
                }
            }
            None => {
                for item in &i.items {
                    if let syn::ImplItem::Fn(f) = item {
                        if is_pub(&f.vis) && !is_cfg_test(&f.attrs) && takes_input(&f.sig) {
                            self.input_fns.push(f.sig.ident.to_string());
                        }
                    }
                }
            }
        }
        syn::visit::visit_item_impl(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if matches!(i.sig.safety, syn::Safety::Unsafe(_)) {
            self.unsafe_sites += 1;
        }
        syn::visit::visit_impl_item_fn(self, i);
    }
}

/// Whether a manifest names a property-testing or fuzzing crate as a dependency key.
fn manifest_names_target(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        TARGET_CRATES.iter().any(|c| {
            t.strip_prefix(c)
                .is_some_and(|rest| rest.trim_start().starts_with(['=', '.']))
        })
    })
}

fn has_target(view: &RepoView, dir: &str) -> bool {
    [dir, ""].iter().any(|d| {
        view.has(&join(d, "fuzz/Cargo.toml"))
            || view
                .read(&join(d, "Cargo.toml"))
                .is_some_and(|t| manifest_names_target(&t))
    })
}

fn judge_package(view: &RepoView, p: &Package) -> Option<Finding> {
    let mut risk = Risk::default();
    for s in &p.sources {
        risk.visit_file(&s.file);
    }
    let by_input = !risk.input_fns.is_empty() || risk.from_str_impls > 0;
    let by_unsafe = risk.unsafe_sites >= UNSAFE_SITES;
    if !(by_input || by_unsafe) || has_target(view, &p.dir) {
        return None;
    }
    let mut reasons = Vec::new();
    if by_input {
        let mut names = risk.input_fns.clone();
        names.sort();
        names.dedup();
        let shown: Vec<String> = names.iter().take(3).map(|n| format!("`{n}`")).collect();
        let mut what = shown.join(", ");
        if risk.from_str_impls > 0 {
            if !what.is_empty() {
                what.push_str(" and ");
            }
            what.push_str(&format!("{} `FromStr` impl(s)", risk.from_str_impls));
        }
        reasons.push(format!("it takes text or bytes to decode ({what})"));
    }
    if by_unsafe {
        reasons.push(format!("it has {} `unsafe` sites", risk.unsafe_sites));
    }
    let manifest = join(&p.dir, "Cargo.toml");
    let name = if p.dir.is_empty() { "." } else { &p.dir };
    Some(
        Finding::new(
            ID,
            Severity::Medium,
            Confidence::Low,
            view.name(),
            format!(
                "package `{name}` has no property or fuzz target and {}",
                reasons.join(" and ")
            ),
            "Input-handling and unsafe code fail on shapes nobody wrote an example for, which generated and mutated input reaches. Add a property test (proptest, quickcheck) or a fuzz target (cargo fuzz) for the decoding or unsafe module, and run it regularly, with longer fuzz runs on a schedule.",
        )
        .location(Location::new(manifest.as_str(), Some(1)))
        .subject(name.to_string())
        .positive_control(format!(
            "{} Rust source file(s) of the package were parsed and its manifest and any `fuzz/Cargo.toml` were looked for",
            p.sources.len()
        )),
    )
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let findings: Vec<Finding> = loaded
        .packages
        .iter()
        .filter_map(|p| judge_package(view, p))
        .collect();
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(manifest: &str, src: &str, extra: &[(&str, &str)]) -> Vec<Finding> {
        let mut files = vec![("Cargo.toml", manifest), ("src/lib.rs", src)];
        files.extend_from_slice(extra);
        match judge(&RepoView::from_files(&files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_parser_with_no_target_is_reported_and_a_proptest_dependency_clears_it() {
        let src = "pub fn parse_header(input: &str) -> u8 { input.len() as u8 }\n";
        let f = found(MANIFEST, src, &[]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("parse_header"), "{}", f[0].summary);
        assert_eq!(f[0].location.as_ref().unwrap().file, "Cargo.toml");
        let with = format!("{MANIFEST}[dev-dependencies]\nproptest = \"1\"\n");
        assert!(found(&with, src, &[]).is_empty());
    }

    #[test]
    fn a_fuzz_directory_in_the_package_or_at_the_root_clears_it() {
        let src = "pub fn decode(b: &[u8]) -> usize { b.len() }\n";
        assert!(found(MANIFEST, src, &[("fuzz/Cargo.toml", "[package]\n")]).is_empty());
        assert_eq!(found(MANIFEST, src, &[]).len(), 1);
    }

    #[test]
    fn unsafe_heavy_packages_and_fromstr_impls_are_risky_and_a_few_unsafe_blocks_are_not() {
        let many = "pub fn f() { unsafe {} unsafe {} unsafe {} unsafe {} unsafe {} }\n";
        let f = found(MANIFEST, many, &[]);
        assert_eq!(f.len(), 1);
        assert!(
            f[0].summary.contains("5 `unsafe` sites"),
            "{}",
            f[0].summary
        );
        assert!(found(MANIFEST, "pub fn f() { unsafe {} unsafe {} }\n", &[]).is_empty());
        let fs = "pub struct S;\nimpl std::str::FromStr for S { type Err = (); fn from_str(_: &str) -> Result<S, ()> { Ok(S) } }\n";
        assert_eq!(found(MANIFEST, fs, &[]).len(), 1);
    }

    #[test]
    fn private_and_non_input_functions_and_test_code_are_not_risky() {
        let src = "fn parse_private(s: &str) {}\npub fn parse_count(n: u32) {}\npub fn render(s: &str) {}\n#[cfg(test)]\nmod t { pub fn parse_x(s: &str) {} }\n";
        assert!(found(MANIFEST, src, &[]).is_empty());
    }

    #[test]
    fn a_dependency_key_is_matched_not_a_substring() {
        assert!(manifest_names_target("proptest = \"1\"\n"));
        assert!(manifest_names_target("arbitrary.workspace = true\n"));
        assert!(!manifest_names_target("name = \"proptest-helper\"\n"));
        assert!(!manifest_names_target("# proptest = \"1\"\n"));
    }

    #[test]
    fn every_finding_validates() {
        for f in found(MANIFEST, "pub fn parse(s: &str) {}\n", &[]) {
            f.validate().unwrap();
        }
    }
}
