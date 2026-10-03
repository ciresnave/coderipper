use crate::cargo_json::{build_all_targets, build_lib_only, CAP_LINTS};
use std::path::Path;

pub struct DeadCodeHit {
    pub file: String,
    pub line: u32,
    pub symbol: String,
}

pub struct CollectResult {
    pub hits: Vec<DeadCodeHit>,
    pub build_failed_for_other_reasons: bool,
}

/// Which targets to build. `LibOnly` is for a package with a library: downgrading the lib's `pub`
/// items breaks every OTHER target that imports it by crate name (E0603), so those must not be built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Targets {
    All,
    LibOnly,
}

pub fn collect_dead_code_in(
    worktree_root: &Path,
    cache_source: Option<&Path>,
    targets: Targets,
) -> anyhow::Result<CollectResult> {
    // `--cap-lints=warn`: a `#![deny(warnings)]` or `[lints]` table must not turn unrelated lints
    // into errors that this would mistake for "the build is broken" (and must not stop cargo
    // compiling dependent targets). dead_code stays a warning, which is all this needs.
    let build = match targets {
        Targets::All => build_all_targets(worktree_root, cache_source, CAP_LINTS)?,
        Targets::LibOnly => build_lib_only(worktree_root, cache_source, CAP_LINTS)?,
    };

    let mut hits = Vec::new();
    for d in &build.diagnostics {
        if d.code.as_deref() != Some("dead_code") {
            continue;
        }
        // Grouped diagnostics ("methods `a`, `b`, and `c` are never used") carry one PRIMARY
        // span per symbol, in the same order as the backtick-quoted names in the message text.
        // Taking only the first silently loses every symbol after it.
        let names = extract_symbol_names(&d.message);
        let primary_spans: Vec<_> = d.spans.iter().filter(|s| s.is_primary).collect();
        for (span, name) in primary_spans.iter().zip(names.iter()) {
            hits.push(DeadCodeHit {
                file: span.file.clone(),
                line: span.line,
                symbol: name.clone(),
            });
        }
    }

    // The same dead_code site is reported once per compilation of the shared code; dedupe by
    // (file, line, symbol) -- that triple uniquely identifies one diagnostic site.
    hits.sort_by(|a, b| (&a.file, a.line, &a.symbol).cmp(&(&b.file, b.line, &b.symbol)));
    hits.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.symbol == b.symbol);

    Ok(CollectResult {
        hits,
        build_failed_for_other_reasons: build.is_broken(),
    })
}

/// rustc's dead_code message is like: "function `dead` is never used", or, for a grouped
/// diagnostic, "methods `a`, `b`, and `c` are never used" -- pull every backtick-quoted name, in
/// the order they appear (which matches the order of the message's primary spans).
fn extract_symbol_names(message: &str) -> Vec<String> {
    message
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{collect_dead_code_in, CollectResult, Targets};
    use std::path::Path;

    fn write_fixture(dir: &Path, lib_rs: &str) {
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), lib_rs).unwrap();
    }

    #[test]
    fn a_dead_pub_crate_item_is_reported_with_its_real_location() {
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(tmp.path(), "pub(crate) fn dead() {}\n");

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();

        assert!(!build_failed_for_other_reasons);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].symbol, "dead");
        assert!(hits[0].file.ends_with("lib.rs"));
    }

    #[test]
    fn a_used_pub_crate_item_produces_no_hit_the_positive_control() {
        // The positive control: a genuinely-`pub` caller (NOT pub(crate) -- Task 4's fixtures don't
        // go through Task 3's rewriter, so this is real, unmodified pub-ness) invoking a pub(crate)
        // callee. Real pub items are rustc's OWN dead_code exemption -- the exact mechanism the whole
        // check exploits -- so this is the correct way to prove the pipeline can see a real caller.
        //
        // NOTE, found empirically while debugging this task (see the ledger): a function called only
        // from a #[test] is NOT rescued from dead_code by rustc (confirmed across four independent,
        // fully-isolated fresh builds on rustc 1.98.1, down to the simplest possible case) -- so a
        // #[test]-based positive control does not work, and "a test counts as reachability" (design
        // doc §2) is not yet delivered by this mechanism for a pure-library project with no real pub
        // surface. Flagged for the PR description as a real, known gap, not silently worked around.
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(
            tmp.path(),
            "pub(crate) fn used() -> i32 { 1 }\npub fn caller() -> i32 { used() }\n",
        );

        let CollectResult { hits, .. } =
            collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();
        assert!(
            hits.is_empty(),
            "the pipeline must be able to see a real caller when one exists"
        );
    }

    #[test]
    fn a_grouped_diagnostic_reports_every_symbol_not_just_the_first() {
        // Review finding: rustc groups multiple unused impl methods into ONE message ("methods
        // `a`, `b`, and `c` are never used") with one primary span per method, in the same order as
        // the backtick-quoted names in the message. The old code took only the first primary span
        // (via .find()), silently losing `b` and `c`.
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(
            tmp.path(),
            "pub(crate) struct Foo;\nimpl Foo {\n    fn a(&self) {}\n    fn b(&self) {}\n    fn c(&self) {}\n}\n",
        );

        let CollectResult { hits, .. } =
            collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();
        let mut symbols: Vec<&str> = hits.iter().map(|h| h.symbol.as_str()).collect();
        symbols.sort();
        assert_eq!(symbols, vec!["Foo", "a", "b", "c"]);
    }

    #[test]
    fn dead_code_promoted_to_an_error_by_deny_warnings_is_still_reported_as_a_finding() {
        // Review finding: a crate-level #![deny(warnings)] escalates dead_code's level to "error".
        // The old code treated ANY error-level message as a sign the whole build was broken and
        // silently discarded everything, including this genuinely real, actionable finding.
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(tmp.path(), "#![deny(warnings)]\npub(crate) fn dead() {}\n");

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();

        assert!(!build_failed_for_other_reasons);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].symbol, "dead");
    }

    #[test]
    fn another_denied_lint_is_not_mistaken_for_a_broken_build() {
        // `#![deny(warnings)]` turns EVERY lint into an error, e.g. an unused import. The old parser
        // treated any non-dead_code error as "the build is broken" and made the whole check error;
        // `--cap-lints=warn` keeps them warnings, so only a real compiler error counts.
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(
            tmp.path(),
            "#![deny(warnings)]\nuse std::collections::HashMap;\npub(crate) fn dead() {}\n",
        );

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();

        assert!(!build_failed_for_other_reasons);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].symbol, "dead");
    }

    #[test]
    fn a_build_rs_panic_with_no_compiler_message_is_still_flagged_as_a_failure() {
        // Review finding: a panicking build.rs writes plain text to stderr, not rustc JSON --
        // `saw_error` (only ever set from a parsed compiler-message) stayed false even though the
        // build genuinely failed, so this case fell through as "0 findings, not failed" (silent
        // false-clean) rather than being flagged.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("build.rs"),
            "fn main() { panic!(\"boom\"); }\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "pub(crate) fn dead() {}\n").unwrap();

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();

        assert!(build_failed_for_other_reasons);
        assert!(hits.is_empty());
    }

    #[test]
    fn a_crate_that_fails_to_compile_for_other_reasons_is_flagged_not_reported_as_dead_code() {
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(tmp.path(), "this is not valid rust syntax {{{\n");

        let CollectResult {
            hits,
            build_failed_for_other_reasons,
        } = collect_dead_code_in(tmp.path(), None, Targets::All).unwrap();

        assert!(build_failed_for_other_reasons);
        assert!(
            hits.is_empty(),
            "no dead-code claim should be made about a crate that didn't compile"
        );
    }
}
