use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RewrittenWorktree {
    pub root: PathBuf,
    source_repo: PathBuf,
    _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
}

impl RewrittenWorktree {
    /// Creates a detached worktree of `project_root`'s HEAD and rewrites every `.rs` file under its
    /// `src/` with `rewrite(relative_path, source)`. `relative_path` is `src/...` with forward
    /// slashes. Files are visited in sorted order so a stateful `rewrite` (one that hands out ids)
    /// is deterministic. A rewrite error drops the worktree again before returning.
    pub fn create_with<F>(project_root: &Path, mut rewrite: F) -> anyhow::Result<Self>
    where
        F: FnMut(&str, &str) -> anyhow::Result<String>,
    {
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
            root: wt_path.clone(),
            source_repo: project_root.to_path_buf(),
            _scratch: scratch,
        };

        let mut files = walk_rs_files(&wt_path.join("src"))?;
        files.sort();
        for entry in files {
            let relative = entry
                .strip_prefix(&wt_path)?
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
            .arg(&self.root)
            .current_dir(&self.source_repo)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !removed {
            // Source repo may itself be gone (e.g. a test's own tempdir already dropped) -- fall
            // back to a plain directory removal so we don't leak disk space either way.
            let _ = std::fs::remove_dir_all(&self.root);
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
