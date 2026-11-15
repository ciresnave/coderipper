// SPDX-License-Identifier: MIT OR Apache-2.0
//! COR-006: unsafe confined and justified, the static half (Rust).
//!
//! **What this judges (Rust, from the source alone):** the rule says Rust code should ban `unsafe` outright, or keep it to compact areas
//! that each carry a written justification, and run the tests under Miri. This is the first half, read from the same inventory `SEC-005`
//! takes (`sec005::inventory`: one scan, two rules, so they cannot disagree). Per Rust package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) a finding is reported when it has an `unsafe` block or an
//! `unsafe impl` / `unsafe trait` with no `SAFETY` comment on its line or in the comment lines directly above it. A package with no
//! `unsafe` at all passes (it is the "banned outright" case, whether or not it forbids it), and so does one whose every block and
//! impl is justified.
//!
//! **The difference from `SEC-005`.** `SEC-005` reports an inventory at Info and prompts packages with no `unsafe` to forbid it;
//! `COR-006` reports only a package that has unjustified `unsafe`, at the catalog's High severity, Medium confidence.
//!
//! **Not built: the Miri half.** The rule also asks that the test suite is executed under Miri or a sanitizer. That is a run of the
//! project's tests on a nightly toolchain, not something a reader of source can judge, so it is not built here: a pass on this check says
//! nothing about whether the suite was run under Miri. An `unsafe fn` is counted by `SEC-005`, not asked for a comment here (its
//! contract belongs in a `# Safety` doc section).
//!
//! **What it cannot see:** `unsafe` inside a macro's arguments or in macro-generated code; native code in another language; a comment
//! that says `SAFETY` and says nothing useful; whether a block is sound. A file `syn` cannot parse is named in a note, never read as
//! clean.

use super::rust_package::{load, unparsed_note};
use super::sec005::inventory;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::COR_006;

/// How many undocumented sites the finding lists.
const LISTED: usize = 8;

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let inv = inventory(view, p);
        let undocumented = inv.undocumented();
        let Some(first) = undocumented.first() else {
            continue;
        };
        let listed: Vec<String> = undocumented
            .iter()
            .take(LISTED)
            .map(|(path, line, _)| format!("{path}:{line}"))
            .collect();
        let more = undocumented.len().saturating_sub(listed.len());
        let summary = format!(
            "package `{}` has {} `unsafe` block(s) or impl(s) with no `SAFETY` comment",
            inv.name,
            undocumented.len()
        );
        let detail = format!(
            "Without a `// SAFETY:` comment directly above (or on the line): {}{}. Forbid `unsafe` (`#![forbid(unsafe_code)]`) or state why each \
             site is sound, as clippy's `undocumented_unsafe_blocks` also asks. This reads the source only: whether the tests are run under Miri \
             or a sanitizer, the other half of the rule, is not judged.",
            listed.join(", "),
            if more == 0 {
                String::new()
            } else {
                format!(", and {more} more")
            }
        );
        findings.push(
            Finding::new(
                ID,
                Severity::High,
                Confidence::Medium,
                view.name(),
                summary,
                detail,
            )
            .location(Location::new(first.0.as_str(), Some(first.1)))
            .subject(inv.name.clone())
            .positive_control(format!(
                "{} Rust source file(s) of the package were parsed with the same walk that counts `unsafe` for SEC-005; {} block(s), {} unsafe fn(s), {} unsafe impl(s) were read",
                inv.files,
                inv.blocks.len(),
                inv.fns,
                inv.impl_sites.len()
            )),
        );
    }
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
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
    fn an_unsafe_block_without_a_safety_comment_is_reported_once_per_package() {
        let src = "fn a() {\n    unsafe { x() };\n}\nfn b() {\n    unsafe { y() };\n}\n";
        let f = found(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::High);
        assert!(
            f[0].summary.contains("2 `unsafe` block(s)"),
            "{}",
            f[0].summary
        );
    }

    #[test]
    fn a_safety_comment_above_or_on_the_line_clears_a_block() {
        let src = "fn a() {\n    // SAFETY: x is sound here\n    unsafe { x() };\n}\nfn b() {\n    unsafe { y() }; // SAFETY: y too\n}\n";
        assert!(found(src).is_empty());
    }

    #[test]
    fn an_unsafe_impl_without_a_comment_is_reported() {
        let src = "struct S;\nunsafe impl Send for S {}\n";
        assert_eq!(found(src).len(), 1);
    }

    #[test]
    fn a_package_with_no_unsafe_and_an_unsafe_fn_alone_pass() {
        assert!(found("fn a() {}\n").is_empty());
        assert!(found("unsafe fn c() {}\n").is_empty());
    }
}
