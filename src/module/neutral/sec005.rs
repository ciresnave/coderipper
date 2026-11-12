// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-005: an inventory of `unsafe` per Rust package, and a prompt to forbid it where a package has none.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`, and its `unsafe` blocks, `unsafe fn`s
//! (free functions, methods and trait methods) and `unsafe impl`/`unsafe trait` items are counted. One **Info** finding per package says
//! how many of each there are and lists the blocks and `unsafe impl`s that have no `SAFETY` comment on the same line or in the comment
//! lines directly above (the standard `// SAFETY:` convention, which clippy's `undocumented_unsafe_blocks` also asks for). A package
//! that has no `unsafe` at all and does not forbid it (no `forbid(unsafe_code)` or `deny(unsafe_code)` in a `lib.rs` or `main.rs` of the
//! package, and no `unsafe_code` lint in its `Cargo.toml` or an enclosing one) gets one Info finding saying so: forbidding costs one
//! line and keeps it that way.
//!
//! **Shared with COR-006:** COR-006's static half (unsafe banned, or each block justified) reads this same inventory (`inventory`,
//! below), so the two cannot disagree; this check does not itself claim COR-006. An `unsafe fn` is counted, not required to have a comment (its contract belongs in a `# Safety` doc section).
//!
//! **What it cannot see:** `unsafe` inside a macro's arguments or in macro-generated code; native code in another language (C, C++ built by
//! `build.rs` or linked as a library); a comment that says `SAFETY` and says nothing useful; whether a block is sound. A file `syn`
//! cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note, Package};
use super::{join, RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::SEC_005;

/// How many undocumented sites the finding lists.
const LISTED: usize = 8;

#[derive(Default)]
struct Sites {
    blocks: Vec<u32>,
    fns: Vec<u32>,
    impls: Vec<u32>,
}

fn line_of(span: proc_macro2::Span) -> u32 {
    u32::try_from(span.start().line).unwrap_or(0)
}

impl<'ast> Visit<'ast> for Sites {
    fn visit_expr_unsafe(&mut self, e: &'ast syn::ExprUnsafe) {
        self.blocks.push(line_of(e.span()));
        syn::visit::visit_expr_unsafe(self, e);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if matches!(i.sig.safety, syn::Safety::Unsafe(_)) {
            self.fns.push(line_of(i.sig.span()));
        }
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if matches!(i.sig.safety, syn::Safety::Unsafe(_)) {
            self.fns.push(line_of(i.sig.span()));
        }
        syn::visit::visit_impl_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if matches!(i.sig.safety, syn::Safety::Unsafe(_)) {
            self.fns.push(line_of(i.sig.span()));
        }
        syn::visit::visit_trait_item_fn(self, i);
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if i.unsafety.is_some() {
            self.impls.push(line_of(i.span()));
        }
        syn::visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        if i.unsafety.is_some() {
            self.impls.push(line_of(i.span()));
        }
        syn::visit::visit_item_trait(self, i);
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }
}

/// Whether the site at 1-based `line` has a `SAFETY` comment: on the line itself, or in the run of comment and attribute lines
/// directly above it.
fn justified(lines: &[&str], line: u32) -> bool {
    let Some(idx) = (line as usize).checked_sub(1) else {
        return false;
    };
    if lines.get(idx).is_some_and(|l| l.contains("SAFETY")) {
        return true;
    }
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines.get(i).map_or("", |l| l.trim());
        if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.ends_with("*/") {
            if t.contains("SAFETY") {
                return true;
            }
        } else if !t.starts_with("#[") {
            break;
        }
    }
    false
}

/// Whether the package forbids or denies `unsafe_code`: in a crate root, or in its manifest or an enclosing one (`[lints.rust]`).
fn forbids_unsafe(view: &RepoView, p: &Package) -> bool {
    let in_root = p.sources.iter().any(|s| {
        let name = s.path.rsplit('/').next().unwrap_or("");
        (name == "lib.rs" || name == "main.rs")
            && (s.text.contains("forbid(unsafe_code)") || s.text.contains("deny(unsafe_code)"))
    });
    if in_root {
        return true;
    }
    let mut dir = p.dir.as_str();
    loop {
        if view
            .read(&join(dir, "Cargo.toml"))
            .is_some_and(|t| t.contains("unsafe_code"))
        {
            return true;
        }
        if dir.is_empty() {
            return false;
        }
        dir = dir.rsplit_once('/').map_or("", |(d, _)| d);
    }
}

/// The `unsafe` inventory of one package, read once and shared by `SEC-005` and `COR-006` (one scan, two rules, so they cannot disagree).
pub(super) struct Inventory {
    /// The package directory, or the repository name for a root package.
    pub name: String,
    /// How many Rust source files were parsed.
    pub files: usize,
    /// `(path, line, has a SAFETY comment)` for each `unsafe` block.
    pub blocks: Vec<(String, u32, bool)>,
    /// How many `unsafe fn`s (counted, not asked for a comment).
    pub fns: usize,
    /// `(path, line, has a SAFETY comment)` for each `unsafe impl` and `unsafe trait`.
    pub impl_sites: Vec<(String, u32, bool)>,
}

impl Inventory {
    /// The blocks and `unsafe impl`/`unsafe trait` items with no `SAFETY` comment, blocks first.
    pub fn undocumented(&self) -> Vec<&(String, u32, bool)> {
        self.blocks
            .iter()
            .chain(self.impl_sites.iter())
            .filter(|(_, _, ok)| !ok)
            .collect()
    }
}

pub(super) fn inventory(view: &RepoView, p: &Package) -> Inventory {
    let mut blocks: Vec<(String, u32, bool)> = Vec::new();
    let mut fns = 0usize;
    let mut impl_sites: Vec<(String, u32, bool)> = Vec::new();
    for s in &p.sources {
        let mut sites = Sites::default();
        sites.visit_file(&s.file);
        let lines: Vec<&str> = s.text.lines().collect();
        fns += sites.fns.len();
        for l in sites.blocks {
            blocks.push((s.path.clone(), l, justified(&lines, l)));
        }
        for l in sites.impls {
            impl_sites.push((s.path.clone(), l, justified(&lines, l)));
        }
    }
    let name = if p.dir.is_empty() {
        view.name().to_string()
    } else {
        p.dir.clone()
    };
    Inventory {
        name,
        files: p.sources.len(),
        blocks,
        fns,
        impl_sites,
    }
}

fn judge_package(view: &RepoView, p: &Package) -> Option<Finding> {
    let Inventory {
        name,
        files,
        blocks,
        fns,
        impl_sites,
    } = inventory(view, p);
    let impls = impl_sites.len();
    let control = format!(
        "{files} Rust source file(s) of the package were parsed with the same walk that counts `unsafe`"
    );
    let total = blocks.len() + fns + impls;
    if total == 0 {
        if forbids_unsafe(view, p) {
            return None;
        }
        let first = p.sources.first().map(|s| s.path.clone());
        let summary = format!("package `{name}` uses `unsafe` nowhere and does not forbid it");
        let detail = "Add `#![forbid(unsafe_code)]` to the crate root (or `unsafe_code = \"forbid\"` under `[lints.rust]`): it costs one line and the compiler then keeps the count at zero.";
        let mut f = Finding::new(
            ID,
            Severity::Info,
            Confidence::High,
            view.name(),
            summary,
            detail,
        )
        .subject(name)
        .positive_control(control);
        if let Some(path) = first {
            f = f.location(Location::new(path.as_str(), None));
        }
        return Some(f);
    }
    let undocumented: Vec<&(String, u32, bool)> = blocks
        .iter()
        .chain(impl_sites.iter())
        .filter(|(_, _, ok)| !ok)
        .collect();
    let listed: Vec<String> = undocumented
        .iter()
        .take(LISTED)
        .map(|(p, l, _)| format!("{p}:{l}"))
        .collect();
    let more = undocumented.len().saturating_sub(listed.len());
    if undocumented.is_empty() {
        // every block and impl is justified: the inventory is complete and there is nothing to ask for
        return None;
    }
    let first = undocumented
        .first()
        .copied()
        .or_else(|| blocks.first())
        .or_else(|| impl_sites.first());
    let summary = format!(
        "package `{name}`: {} unsafe block(s), {fns} unsafe fn(s), {impls} unsafe impl/trait item(s); {} block or impl site(s) lack a `SAFETY` comment",
        blocks.len(),
        undocumented.len()
    );
    let detail = {
        format!(
            "Without a `// SAFETY:` comment directly above (or on the line): {}{}. State why each is sound, as clippy's `undocumented_unsafe_blocks` also asks. An `unsafe fn` is counted but its contract belongs in a `# Safety` doc section.",
            listed.join(", "),
            if more == 0 {
                String::new()
            } else {
                format!(", and {more} more")
            }
        )
    };
    let mut f = Finding::new(
        ID,
        Severity::Info,
        Confidence::High,
        view.name(),
        summary,
        detail,
    )
    .subject(name)
    .positive_control(control);
    if let Some((path, line, _)) = first {
        f = f.location(Location::new(path.as_str(), Some(*line)));
    }
    Some(f)
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

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    #[test]
    fn an_unsafe_block_without_a_safety_comment_is_listed_and_one_with_it_is_not() {
        let src = "fn a() {\n    unsafe { x() };\n}\nfn b() {\n    // SAFETY: x is sound here\n    unsafe { x() };\n}\nunsafe fn c() {}\n";
        let f = found(&[("Cargo.toml", MANIFEST), ("src/lib.rs", src)]);
        assert_eq!(f.len(), 1);
        assert!(
            f[0].summary.contains("2 unsafe block(s), 1 unsafe fn(s)"),
            "{}",
            f[0].summary
        );
        assert!(
            f[0].summary.contains("1 block or impl site(s) lack"),
            "{}",
            f[0].summary
        );
        assert!(f[0].detail.contains("src/lib.rs:2"), "{}", f[0].detail);
        assert!(!f[0].detail.contains("src/lib.rs:6"), "{}", f[0].detail);
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(2));
    }

    #[test]
    fn a_comment_through_an_attribute_line_still_counts() {
        let src = "fn b() {\n    // SAFETY: fine\n    #[allow(unused)]\n    let _v = unsafe { x() };\n}\n";
        let f = found(&[("Cargo.toml", MANIFEST), ("src/lib.rs", src)]);
        // justified through the attribute line: the package is complete and says nothing
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn unsafe_impl_and_unsafe_trait_are_counted() {
        let src = "struct S;\n// SAFETY: S holds no pointers\nunsafe impl Send for S {}\nunsafe trait T {}\n";
        let f = found(&[("Cargo.toml", MANIFEST), ("src/lib.rs", src)]);
        assert!(
            f[0].summary.contains("2 unsafe impl/trait item(s)"),
            "{}",
            f[0].summary
        );
        assert!(
            f[0].summary.contains("1 block or impl site(s) lack"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn test_code_is_not_counted() {
        let src = "fn a() {}\n#[cfg(test)]\nmod t { fn g() { unsafe { x() }; } }\n";
        let f = found(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
            ("tests/x.rs", "fn g() { unsafe { x() }; }\n"),
        ]);
        assert!(
            f[0].summary.contains("uses `unsafe` nowhere"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn a_package_with_none_is_told_to_forbid_it_and_one_that_does_says_nothing() {
        let plain = found(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "pub fn a() {}\n")]);
        assert!(
            plain[0].summary.ends_with("does not forbid it"),
            "{}",
            plain[0].summary
        );
        let forbidden = found(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", "#![forbid(unsafe_code)]\npub fn a() {}\n"),
        ]);
        assert!(forbidden.is_empty(), "{forbidden:?}");
        let by_lint = found(&[
            (
                "Cargo.toml",
                "[package]\nname=\"p\"\n[lints.rust]\nunsafe_code = \"forbid\"\n",
            ),
            ("src/lib.rs", "pub fn a() {}\n"),
        ]);
        assert!(by_lint.is_empty(), "{by_lint:?}");
    }

    #[test]
    fn each_package_of_a_workspace_is_its_own_finding() {
        let f = found(&[
            ("Cargo.toml", "[workspace]\nmembers=[\"a\",\"b\"]\n"),
            ("a/Cargo.toml", MANIFEST),
            ("a/src/lib.rs", "pub fn a() { unsafe { x() }; }\n"),
            ("b/Cargo.toml", MANIFEST),
            ("b/src/lib.rs", "pub fn b() {}\n"),
        ]);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].subject.as_deref(), Some("a"));
        assert_eq!(f[1].subject.as_deref(), Some("b"));
    }

    #[test]
    fn every_finding_validates_and_a_file_without_a_package_is_ignored() {
        let f = found(&[
            ("a/Cargo.toml", MANIFEST),
            ("a/src/lib.rs", "pub fn a() {}\n"),
            ("loose/x.rs", "fn g() { unsafe { x() }; }\n"),
        ]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(
            f[0].summary.contains("uses `unsafe` nowhere"),
            "{}",
            f[0].summary
        );
        for finding in f {
            finding.validate().unwrap();
        }
    }

    #[test]
    fn a_repository_with_no_rust_package_does_not_apply() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
    }
}
