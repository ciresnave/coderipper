// SPDX-License-Identifier: MIT OR Apache-2.0
use super::classify::SENTINEL_TAG;
use std::path::Path;

/// Not underscore-prefixed on purpose: rustc treats a leading `_` as "intentionally unused" and
/// would silence the very lints the sentinel exists to prove are working.
pub const SENTINEL_FN: &str = "coderipper_unused_return_sentinel_7d41";

fn append_sentinel(path: &Path) -> anyhow::Result<()> {
    let mut content = std::fs::read_to_string(path)?;
    content.push_str(&format!(
        "\n#[allow(dead_code)]\n\
         #[must_use = \"CR:{SENTINEL_TAG}\"] #[deprecated(note = \"CR:{SENTINEL_TAG}\")]\n\
         fn {SENTINEL_FN}() -> i32 {{ 1 }}\n\
         #[allow(dead_code)]\n\
         fn {SENTINEL_FN}_caller() {{ {SENTINEL_FN}(); }}\n"
    ));
    std::fs::write(path, content)?;
    Ok(())
}

/// Appends a function tagged exactly like every real one, plus a caller that discards its value,
/// to `src/lib.rs` (preferred) or `src/main.rs`. If this run's build does not report BOTH the
/// ignored call and the use of this function, the crate is suppressing one of the two lints
/// (`#![allow(deprecated)]`, `#![allow(unused_must_use)]`) and no real result can be trusted.
/// Returns the file it wrote to.
pub fn inject_sentinel(worktree_root: &Path) -> anyhow::Result<String> {
    for candidate in ["lib.rs", "main.rs"] {
        let path = worktree_root.join("src").join(candidate);
        if path.exists() {
            append_sentinel(&path)?;
            return Ok(format!("src/{candidate}"));
        }
    }
    // no `src/lib.rs` or `src/main.rs`: the first library or binary root cargo reports (a `[[bin]]` with a `path` outside `src/`)
    if let Some(relative) = crate::package::root_files(worktree_root)
        .ok()
        .and_then(|roots| roots.into_iter().next())
    {
        append_sentinel(&worktree_root.join(&relative))?;
        return Ok(relative);
    }
    anyhow::bail!("neither src/lib.rs nor src/main.rs exists under {worktree_root:?}, and cargo reports no target root")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_with(file: &str, contents: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src").join(file), contents).unwrap();
        tmp
    }

    #[test]
    fn it_appends_to_lib_rs_and_keeps_the_original_content() {
        let tmp = crate_with("lib.rs", "pub fn real() {}\n");
        assert_eq!(inject_sentinel(tmp.path()).unwrap(), "src/lib.rs");
        let text = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(text.starts_with("pub fn real() {}\n"));
        assert!(text.contains(SENTINEL_FN) && text.contains("CR:sentinel"));
    }

    #[test]
    fn it_falls_back_to_main_rs() {
        let tmp = crate_with("main.rs", "fn main() {}\n");
        assert_eq!(inject_sentinel(tmp.path()).unwrap(), "src/main.rs");
    }

    #[test]
    fn it_fails_when_there_is_no_root_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        assert!(inject_sentinel(tmp.path()).is_err());
    }
}
