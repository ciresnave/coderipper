// SPDX-License-Identifier: MIT OR Apache-2.0
//! CON-010: an atomic operation that uses an ordering weaker than `SeqCst` carries a comment.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of every package (the shared `rust_package::load` walk,
//! so vendored directories, tests, examples, tools and `build.rs` are outside), parsed with `syn`, outside `#[cfg(test)]` modules and
//! test functions. A use of `Ordering::Relaxed`, `Ordering::Acquire`, `Ordering::Release` or `Ordering::AcqRel` (any path ending in
//! `Ordering` and one of those names, so `atomic::Ordering::Relaxed` and `std::sync::atomic::Ordering::Release` count) is a **site**.
//! A site is **justified** when a comment is on its line or on any of the three lines above it. A package with undocumented sites gets
//! one finding, listing the first of them, at the first. The text of the comment is not judged, only that one is there.
//!
//! **What it cannot see:** an ordering imported by name (`use Ordering::*;` then a bare `Relaxed`), an ordering held in a constant or
//! passed through a variable, an ordering named in a macro's arguments (`syn` does not look into them), whether a comment actually
//! argues for the ordering, whether the ordering is in fact enough, other languages. A file `syn` cannot parse is named in a note, never
//! read as clean. Low severity, Low confidence: whether a comment justifies an ordering needs a reader.

use super::rust_package::{load, unparsed_note, Package};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::CON_010;

/// How many undocumented sites the finding lists.
const LISTED: usize = 8;

/// How many lines above a site a comment may sit and still count.
const REACH: usize = 3;

const WEAK: &[&str] = &["Relaxed", "Acquire", "Release", "AcqRel"];

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

#[derive(Default)]
struct Walk {
    /// (line, ordering name)
    sites: Vec<(u32, String)>,
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
        syn::visit::visit_item_fn(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        syn::visit::visit_impl_item_fn(self, i);
    }

    fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
        let segs: Vec<String> = e
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        if let [.., owner, name] = segs.as_slice() {
            if owner == "Ordering" && WEAK.contains(&name.as_str()) {
                let line = u32::try_from(e.span().start().line).unwrap_or(0);
                self.sites.push((line, name.clone()));
            }
        }
        syn::visit::visit_expr_path(self, e);
    }
}

/// Whether `line` (1-based) holds a comment, whether on a line of its own or after code.
fn has_comment(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("//")
        || t.starts_with("/*")
        || t.starts_with('*')
        || text.contains("//")
        || text.contains("/*")
}

fn justified(lines: &[&str], line: u32) -> bool {
    let Some(idx) = (line as usize).checked_sub(1) else {
        return false;
    };
    let from = idx.saturating_sub(REACH);
    (from..=idx).any(|i| lines.get(i).is_some_and(|l| has_comment(l)))
}

fn judge_package(view: &RepoView, p: &Package) -> Option<Finding> {
    let mut all = 0usize;
    let mut bare: Vec<(String, u32, String)> = Vec::new();
    for s in &p.sources {
        let mut walk = Walk::default();
        walk.visit_file(&s.file);
        let lines: Vec<&str> = s.text.lines().collect();
        all += walk.sites.len();
        for (line, name) in walk.sites {
            if !justified(&lines, line) {
                bare.push((s.path.clone(), line, name));
            }
        }
    }
    if bare.is_empty() {
        return None;
    }
    bare.sort();
    let name = if p.dir.is_empty() {
        view.name().to_string()
    } else {
        p.dir.clone()
    };
    let listed: Vec<String> = bare
        .iter()
        .take(LISTED)
        .map(|(f, l, o)| format!("{f}:{l} ({o})"))
        .collect();
    let more = bare.len() - listed.len();
    let (file, line, _) = &bare[0];
    Some(
        Finding::new(
            ID,
            Severity::Low,
            Confidence::Low,
            view.name(),
            format!(
                "package `{name}`: {} of {all} weak atomic ordering use(s) have no comment on the line or the {REACH} lines above",
                bare.len()
            ),
            format!(
                "Orderings weaker than `SeqCst` without a comment: {}{}. Say next to each why that ordering is enough (what it synchronises with), or use `SeqCst`; model-check the algorithm (loom) where the stakes are high. Only the presence of a comment is read, not its argument.",
                listed.join(", "),
                if more == 0 {
                    String::new()
                } else {
                    format!(", and {more} more")
                }
            ),
        )
        .location(Location::new(file.as_str(), Some(*line)))
        .subject(name)
        .positive_control(format!(
            "{all} weak ordering use(s) in {} Rust source file(s) of the package were found with the same walk",
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

    fn run(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn an_uncommented_weak_ordering_is_reported_once_per_package() {
        let src = "use std::sync::atomic::{AtomicUsize, Ordering};\nstatic N: AtomicUsize = AtomicUsize::new(0);\npub fn a() -> usize {\n    N.fetch_add(1, Ordering::Relaxed);\n    N.load(std::sync::atomic::Ordering::Acquire)\n}\n";
        let f = run(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(4));
        assert!(f[0].summary.contains("2 of 2"), "{}", f[0].summary);
    }

    #[test]
    fn a_comment_on_the_line_or_just_above_justifies() {
        let src = "use std::sync::atomic::{AtomicUsize, Ordering};\nstatic N: AtomicUsize = AtomicUsize::new(0);\npub fn a() {\n    // a counter: no other data hangs off it\n    N.fetch_add(1, Ordering::Relaxed);\n    N.store(0, Ordering::Release); // pairs with the Acquire in b\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn seqcst_tests_and_far_comments_are_judged_correctly() {
        let src = "use std::sync::atomic::{AtomicUsize, Ordering};\nstatic N: AtomicUsize = AtomicUsize::new(0);\npub fn a() { N.store(1, Ordering::SeqCst); let _ = std::cmp::Ordering::Less; }\n#[cfg(test)] mod t { fn x() { super::N.load(super::Ordering::Relaxed); } }\n#[test] fn u() { super::N.load(super::Ordering::Relaxed); }\n";
        assert!(run(src).is_empty());
        let far = "// the counter\n\n\n\n\npub fn a(n: &std::sync::atomic::AtomicUsize) { n.load(std::sync::atomic::Ordering::Relaxed); }\n";
        assert_eq!(run(far).len(), 1);
    }
}
