use std::path::Path;

/// Neither name starts with `_`: rustc exempts underscore-prefixed names from `unused_variables`,
/// which would silence the very lint the sentinel exists to prove is live.
pub const SENTINEL_FN: &str = "coderipper_unused_param_sentinel_7d41";
pub const SENTINEL_ARG: &str = "coderipper_unused_param_arg_7d41";

/// Appends a function with one unused parameter to EVERY target root: `src/lib.rs`, `src/main.rs`,
/// `src/bin/*.rs` and `src/bin/*/main.rs`. Each target is its own crate with its own crate-level
/// attributes, so a sentinel in one proves nothing about another. If this run's build does not report
/// the sentinel of some root, `unused_variables` is suppressed in that crate and its result can't be
/// trusted. Returns the files written to, as `src/...` paths, sorted.
pub fn inject_sentinels(worktree_root: &Path) -> anyhow::Result<Vec<String>> {
    let src = worktree_root.join("src");
    let mut roots = vec!["lib.rs".to_string(), "main.rs".to_string()];
    if let Ok(entries) = std::fs::read_dir(src.join("bin")) {
        for entry in entries {
            let path = entry?.path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_file() && name.ends_with(".rs") {
                roots.push(format!("bin/{name}"));
            } else if path.is_dir() && path.join("main.rs").is_file() {
                roots.push(format!("bin/{name}/main.rs"));
            }
        }
    }
    roots.sort();

    let mut written = Vec::new();
    for root in roots {
        let path = src.join(&root);
        if path.is_file() {
            let mut content = std::fs::read_to_string(&path)?;
            content.push_str(&format!(
                "\n#[allow(dead_code)]\nfn {SENTINEL_FN}({SENTINEL_ARG}: i32) {{}}\n"
            ));
            std::fs::write(&path, content)?;
            written.push(format!("src/{root}"));
        }
    }
    anyhow::ensure!(
        !written.is_empty(),
        "no target root (src/lib.rs, src/main.rs, src/bin/*) exists under {worktree_root:?}"
    );
    Ok(written)
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
    fn it_appends_to_every_target_root_and_keeps_the_original_content() {
        let tmp = crate_with("lib.rs", "pub fn real() {}\n");
        std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("src/bin/tool")).unwrap();
        std::fs::write(tmp.path().join("src/bin/a.rs"), "fn main() {}\n").unwrap();
        std::fs::write(tmp.path().join("src/bin/tool/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(tmp.path().join("src/bin/not_a_root.txt"), "x").unwrap();

        let files = inject_sentinels(tmp.path()).unwrap();
        assert_eq!(
            files,
            vec![
                "src/bin/a.rs",
                "src/bin/tool/main.rs",
                "src/lib.rs",
                "src/main.rs"
            ]
        );
        let lib = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert!(lib.starts_with("pub fn real() {}\n"));
        assert!(lib.contains(SENTINEL_FN) && lib.contains(SENTINEL_ARG));
        let note = std::fs::read_to_string(tmp.path().join("src/bin/not_a_root.txt")).unwrap();
        assert_eq!(note, "x");
    }

    #[test]
    fn a_package_with_only_bin_targets_still_gets_a_sentinel() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/bin")).unwrap();
        std::fs::write(tmp.path().join("src/bin/a.rs"), "fn main() {}\n").unwrap();
        assert_eq!(inject_sentinels(tmp.path()).unwrap(), vec!["src/bin/a.rs"]);
    }

    #[test]
    fn it_fails_when_there_is_no_target_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        assert!(inject_sentinels(tmp.path()).is_err());
    }
}
