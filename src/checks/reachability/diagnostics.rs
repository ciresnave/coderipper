use serde::Deserialize;
use std::path::Path;
use std::process::Command;

pub struct DeadCodeHit {
    pub file: String,
    pub line: u32,
    pub symbol: String,
}

pub struct CollectResult {
    pub hits: Vec<DeadCodeHit>,
    pub build_failed_for_other_reasons: bool,
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
struct CompilerMessage {
    code: Option<CompilerCode>,
    level: String,
    message: String,
    spans: Vec<CompilerSpan>,
}

#[derive(Deserialize)]
struct CompilerCode {
    code: String,
}

#[derive(Deserialize)]
struct CompilerSpan {
    file_name: String,
    line_start: u32,
    is_primary: bool,
}

pub fn collect_dead_code(worktree_root: &Path) -> anyhow::Result<CollectResult> {
    let output = Command::new("cargo")
        .args(["build", "--all-targets", "--message-format=json"])
        .current_dir(worktree_root)
        .output()?;

    let mut hits = Vec::new();
    let mut saw_error = false;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<CargoMessage>(line) else {
            continue;
        };
        if msg.reason != "compiler-message" {
            continue;
        }
        let Some(cm) = msg.message else { continue };

        if cm.level == "error" {
            saw_error = true;
            continue;
        }

        let is_dead_code = cm.code.as_ref().map(|c| c.code == "dead_code").unwrap_or(false);
        if !is_dead_code {
            continue;
        }

        if let Some(span) = cm.spans.iter().find(|s| s.is_primary) {
            hits.push(DeadCodeHit {
                file: span.file_name.clone(),
                line: span.line_start,
                symbol: extract_symbol_name(&cm.message),
            });
        }
    }

    // `--all-targets` compiles the crate more than once (the plain lib, and again for the test
    // harness binary), so the same dead_code site is reported once per compilation. Dedupe by
    // (file, line, symbol) -- that triple uniquely identifies one diagnostic site.
    hits.sort_by(|a, b| (&a.file, a.line, &a.symbol).cmp(&(&b.file, b.line, &b.symbol)));
    hits.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.symbol == b.symbol);

    Ok(CollectResult {
        hits,
        build_failed_for_other_reasons: saw_error,
    })
}

/// rustc's dead_code message is like: "function `dead` is never used" -- pull the backtick-quoted name.
fn extract_symbol_name(message: &str) -> String {
    message.split('`').nth(1).unwrap_or(message).to_string()
}

#[cfg(test)]
mod tests {
    use super::{collect_dead_code, CollectResult};
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

        let CollectResult { hits, build_failed_for_other_reasons } =
            collect_dead_code(tmp.path()).unwrap();

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

        let CollectResult { hits, .. } = collect_dead_code(tmp.path()).unwrap();
        assert!(hits.is_empty(), "the pipeline must be able to see a real caller when one exists");
    }

    #[test]
    fn a_crate_that_fails_to_compile_for_other_reasons_is_flagged_not_reported_as_dead_code() {
        let tmp = tempfile::tempdir().unwrap();
        write_fixture(tmp.path(), "this is not valid rust syntax {{{\n");

        let CollectResult { hits, build_failed_for_other_reasons } =
            collect_dead_code(tmp.path()).unwrap();

        assert!(build_failed_for_other_reasons);
        assert!(hits.is_empty(), "no dead-code claim should be made about a crate that didn't compile");
    }
}
