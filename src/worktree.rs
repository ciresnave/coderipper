use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RewrittenWorktree {
    /// The PACKAGE's directory inside the checkout: the checkout's root for a repository with one
    /// package at its top, `<checkout>/fuel-core` for a workspace member. Everything a check reads,
    /// rewrites or builds is under it.
    pub root: PathBuf,
    /// The whole checkout, which is what `git worktree remove` takes.
    worktree_path: PathBuf,
    source_repo: PathBuf,
    _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
}

impl RewrittenWorktree {
    /// The source repository (the project directory given to [`Self::create_with`]) this checkout was made
    /// from, as opposed to the throwaway checkout itself. The build cache is keyed by it.
    pub fn source_repo(&self) -> &Path {
        &self.source_repo
    }

    /// Creates a detached worktree of the git repository containing `project_root` at HEAD and
    /// rewrites every `.rs` file under the PACKAGE's `src/` with `rewrite(relative_path, source)`.
    /// `project_root` is the package's directory: the repository root, or a workspace member below
    /// it (the whole repository is checked out, so path dependencies exist, but only the package is
    /// rewritten). `relative_path` is `src/...`, relative to the package, with forward slashes. A
    /// directory that is not a package (a virtual workspace root) is refused. Files are visited in sorted order so a stateful `rewrite` (one that hands out ids)
    /// is deterministic. A rewrite error drops the worktree again before returning.
    pub fn create_with<F>(project_root: &Path, mut rewrite: F) -> anyhow::Result<Self>
    where
        F: FnMut(&str, &str) -> anyhow::Result<String>,
    {
        let prefix = crate::package::git_prefix(project_root)?;
        let scratch = tempfile::tempdir()?;
        let wt_path = scratch.path().join("wt");

        let status = Command::new("git")
            .args(["worktree", "add", "--detach", "-q"])
            .arg(&wt_path)
            .arg("HEAD")
            .current_dir(project_root)
            .status()?;
        anyhow::ensure!(status.success(), "git worktree add failed");

        // Build the guard BEFORE rewriting: if a rewrite fails, `Drop` still unregisters the
        // worktree from the source repo instead of leaving a dangling `git worktree list` entry.
        let guard = Self {
            root: wt_path.join(&prefix),
            worktree_path: wt_path,
            source_repo: project_root.to_path_buf(),
            _scratch: scratch,
        };
        anyhow::ensure!(
            guard.root.is_dir(),
            "{} is not in the repository's HEAD commit (is it committed?); the checks analyze HEAD",
            project_root.display()
        );
        crate::package::require_package_as(&guard.root, project_root)?;

        let mut files = walk_rs_files(&guard.root.join("src"))?;
        files.sort();
        for entry in files {
            let relative = entry
                .strip_prefix(&guard.root)?
                .to_string_lossy()
                .replace('\\', "/");
            let source = std::fs::read_to_string(&entry)?;
            std::fs::write(&entry, rewrite(&relative, &source)?)?;
        }

        Ok(guard)
    }
}

impl Drop for RewrittenWorktree {
    fn drop(&mut self) {
        // `git worktree remove` (run from the SOURCE repo, which we now keep a handle to) both
        // deletes the directory AND unregisters it -- unlike a bare rm -rf, which leaves a dangling
        // `git worktree list` entry in the source repo that accumulates across runs. Reviewed
        // finding: in a shared checkout that dangling entry is an unwanted write into a shared .git.
        let removed = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.worktree_path)
            .current_dir(&self.source_repo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !removed {
            // Source repo may itself be gone (e.g. a test's own tempdir already dropped) -- fall
            // back to a plain directory removal so we don't leak disk space either way.
            let _ = std::fs::remove_dir_all(&self.worktree_path);
        }
    }
}

pub(crate) fn walk_rs_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_rs_files(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::RewrittenWorktree;
    use std::process::Command;

    fn pub_to_pub_crate(_file: &str, source: &str) -> anyhow::Result<String> {
        Ok(source.replace("pub fn", "pub(crate) fn"))
    }

    fn init_fixture_repo(dir: &std::path::Path) {
        Command::new("git")
            .arg("init")
            .arg("-q")
            .current_dir(dir)
            .status()
            .unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn dead() {}\n").unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[test]
    fn the_worktree_is_a_rewritten_copy_not_a_mutation_of_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();

        let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
        assert!(rewritten.contains("pub(crate) fn dead"));

        let original = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert_eq!(
            original, "pub fn dead() {}\n",
            "the caller's real tree must be untouched"
        );
    }

    #[test]
    fn the_rewrite_closure_gets_forward_slash_relative_paths_in_sorted_order() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());
        std::fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        std::fs::write(tmp.path().join("src/a.rs"), "").unwrap();
        std::fs::write(tmp.path().join("src/sub/z.rs"), "").unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "more",
            ])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let mut seen = Vec::new();
        let _wt = RewrittenWorktree::create_with(tmp.path(), |file, source| {
            seen.push(file.to_string());
            Ok(source.to_string())
        })
        .unwrap();

        assert_eq!(seen, vec!["src/a.rs", "src/lib.rs", "src/sub/z.rs"]);
    }

    #[test]
    fn a_failing_rewrite_still_unregisters_the_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let result = RewrittenWorktree::create_with(tmp.path(), |_, _| anyhow::bail!("boom"));
        assert!(result.is_err());

        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "dangling entry:
{listing}"
        );
    }

    /// A two-member virtual workspace (`a`, `b`) as its own committed git repository.
    fn workspace_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let files = [
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
            ),
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("a/src/lib.rs", "pub fn in_a() {}\n"),
            (
                "b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("b/src/lib.rs", "pub fn in_b() {}\n"),
        ];
        for (name, contents) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        for args in [
            vec!["init", "-q"],
            vec!["add", "-A"],
            vec![
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        ] {
            Command::new("git")
                .args(args)
                .current_dir(tmp.path())
                .status()
                .unwrap();
        }
        tmp
    }

    #[test]
    fn a_workspace_member_is_the_root_and_its_siblings_are_not_rewritten() {
        let repo = workspace_repo();
        let mut seen = Vec::new();
        let wt = RewrittenWorktree::create_with(&repo.path().join("b"), |file, source| {
            seen.push(file.to_string());
            pub_to_pub_crate(file, source)
        })
        .unwrap();

        // the closure sees paths relative to the PACKAGE, and only its files
        assert_eq!(seen, vec!["src/lib.rs"]);
        // `root` is the package directory inside the checkout...
        assert!(wt.root.ends_with("b"), "{:?}", wt.root);
        let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
        assert!(rewritten.contains("pub(crate) fn in_b"));
        // ...the sibling is checked out (it may be a path dependency) but untouched...
        let sibling = std::fs::read_to_string(wt.root.join("../a/src/lib.rs")).unwrap();
        // (git may check the file out with CRLF line endings on Windows)
        assert!(
            sibling.contains("pub fn in_a()") && !sibling.contains("pub(crate)"),
            "{sibling:?}"
        );
        // ...and the caller's own tree is untouched too
        let original = std::fs::read_to_string(repo.path().join("b/src/lib.rs")).unwrap();
        assert_eq!(original, "pub fn in_b() {}\n");
    }

    #[test]
    fn dropping_a_member_worktree_removes_the_whole_checkout_and_unregisters_it() {
        let repo = workspace_repo();
        let top = {
            let wt =
                RewrittenWorktree::create_with(&repo.path().join("b"), pub_to_pub_crate).unwrap();
            wt.root.parent().unwrap().to_path_buf()
        };
        assert!(
            !top.exists(),
            "the whole worktree, not just the member, must be removed"
        );
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "dangling entry:\n{listing}"
        );
    }

    #[test]
    fn a_virtual_workspace_root_is_refused_with_the_members_and_leaves_nothing_behind() {
        let repo = workspace_repo();
        let err = RewrittenWorktree::create_with(repo.path(), pub_to_pub_crate)
            .err()
            .expect("must be refused")
            .to_string();
        assert!(err.contains("not a package directory"), "{err}");
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "dangling entry:\n{listing}"
        );
    }

    #[test]
    fn the_worktree_directory_is_removed_when_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let wt_root = {
            let wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();
            wt.root.clone()
        }; // dropped here

        assert!(!wt_root.exists(), "worktree dir must be cleaned up on drop");
    }

    #[test]
    fn dropping_also_unregisters_the_worktree_from_the_source_repo() {
        // Review finding: only deleting the directory leaves a dangling `git worktree list` entry
        // in the SOURCE repo, which accumulates across runs and, in a shared checkout, is a write
        // into a shared .git nobody asked for. `git worktree remove` (not just rm -rf) is required.
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        {
            let _wt = RewrittenWorktree::create_with(tmp.path(), pub_to_pub_crate).unwrap();
        } // dropped here

        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&output.stdout);
        // Only the source repo's own primary worktree should remain listed.
        assert_eq!(
            listing.matches("worktree ").count(),
            1,
            "expected only the source repo's own entry, got:\n{listing}"
        );
    }
}
