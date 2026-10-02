use std::path::Path;

/// Neither name starts with `_`: rustc exempts underscore-prefixed names from `unused_variables`,
/// which would silence the very lint the sentinel exists to prove is live.
pub const SENTINEL_FN: &str = "coderipper_unused_param_sentinel_7d41";
pub const SENTINEL_ARG: &str = "coderipper_unused_param_arg_7d41";

/// Appends a function with one unused parameter to `src/lib.rs` (preferred) or `src/main.rs`. If
/// this run's build does not report that parameter, `unused_variables` is suppressed crate-wide and
/// no result can be trusted. Returns the file it wrote to.
pub fn inject_sentinel(worktree_root: &Path) -> anyhow::Result<String> {
    for candidate in ["lib.rs", "main.rs"] {
        let path = worktree_root.join("src").join(candidate);
        if path.exists() {
            let mut content = std::fs::read_to_string(&path)?;
            content.push_str(&format!(
                "\n#[allow(dead_code)]\nfn {SENTINEL_FN}({SENTINEL_ARG}: i32) {{}}\n"
            ));
            std::fs::write(&path, content)?;
            return Ok(format!("src/{candidate}"));
        }
    }
    anyhow::bail!("neither src/lib.rs nor src/main.rs exists under {worktree_root:?}")
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
        assert!(text.contains(SENTINEL_FN) && text.contains(SENTINEL_ARG));
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
