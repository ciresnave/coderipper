use std::path::Path;

/// A guaranteed-unique symbol name that is never actually used anywhere. Appended to the rewritten
/// worktree's root file so each run has a REAL, per-run positive control: if this exact symbol
/// doesn't come back as a dead_code hit, something about the pipeline can't be trusted this run
/// (the build failed for some other reason, dead_code is suppressed crate-wide, etc.) -- a fixed
/// narrative string claiming "the pipeline can see a real caller" would be a lie in that case.
/// Deliberately does NOT start with `_` -- rustc's dead_code lint treats an underscore-prefixed
/// identifier as "intentionally unused" and never warns about it at all (confirmed empirically:
/// `__coderipper_...` was silently exempt, `coderipper_...` was correctly flagged). A sentinel that
/// can never be detected as dead would make this whole mechanism vacuous.
pub const SENTINEL_SYMBOL: &str = "coderipper_reachability_sentinel_9f3a2b1c";

/// Appends a dead sentinel function to whichever of `src/lib.rs` / `src/main.rs` exists (preferring
/// `lib.rs`), after the pub->pub(crate) rewrite has already run on it. Returns the file it was
/// written to, in the same portable forward-slash form `diagnostics::collect_dead_code` reports.
pub fn inject_sentinel(worktree_root: &Path) -> anyhow::Result<String> {
    for candidate in ["lib.rs", "main.rs"] {
        let path = worktree_root.join("src").join(candidate);
        if path.exists() {
            let mut content = std::fs::read_to_string(&path)?;
            content.push_str(&format!("\npub(crate) fn {SENTINEL_SYMBOL}() {{}}\n"));
            std::fs::write(&path, content)?;
            return Ok(format!("src/{candidate}"));
        }
    }
    anyhow::bail!("neither src/lib.rs nor src/main.rs exists under {worktree_root:?}")
}

#[cfg(test)]
mod tests {
    use super::{inject_sentinel, SENTINEL_SYMBOL};
    use std::path::Path;

    fn write_crate(dir: &Path, root_file: &str, contents: &str) {
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join(root_file), contents).unwrap();
    }

    #[test]
    fn injecting_appends_to_lib_rs_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        write_crate(tmp.path(), "lib.rs", "pub(crate) fn real() {}\n");

        let sentinel_file = inject_sentinel(tmp.path()).unwrap();

        assert_eq!(sentinel_file, "src/lib.rs");
        let content = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(
            content.contains("pub(crate) fn real()"),
            "original content preserved"
        );
        assert!(content.contains(SENTINEL_SYMBOL), "sentinel appended");
    }

    #[test]
    fn injecting_falls_back_to_main_rs_when_no_lib_rs() {
        let tmp = tempfile::tempdir().unwrap();
        write_crate(tmp.path(), "main.rs", "fn main() {}\n");

        let sentinel_file = inject_sentinel(tmp.path()).unwrap();

        assert_eq!(sentinel_file, "src/main.rs");
        let content = std::fs::read_to_string(tmp.path().join("src/main.rs")).unwrap();
        assert!(content.contains(SENTINEL_SYMBOL));
    }

    #[test]
    fn injecting_fails_cleanly_when_neither_root_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();

        assert!(inject_sentinel(tmp.path()).is_err());
    }
}
