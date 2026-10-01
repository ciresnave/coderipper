//! Shared fixture helpers for integration tests. A fixture is built in a throwaway tempdir and
//! committed to its own fresh git repo, because every check reads HEAD through a git worktree --
//! a test must never point a check at the real shared checkout.

use std::path::Path;
use std::process::Command;

pub fn git_repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, contents) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["add", "-A"]);
    git(
        tmp.path(),
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    tmp
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

pub const MANIFEST: &str =
    "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
