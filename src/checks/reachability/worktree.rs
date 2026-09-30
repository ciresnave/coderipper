use super::rewriter::rewrite_pub_to_pub_crate;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RewrittenWorktree {
    pub root: PathBuf,
    source_repo: PathBuf,
    _scratch: tempfile::TempDir, // keeps the parent dir alive for root's lifetime
}

impl RewrittenWorktree {
    pub fn create(project_root: &Path) -> anyhow::Result<Self> {
        let scratch = tempfile::tempdir()?;
        let wt_path = scratch.path().join("wt");

        let status = Command::new("git")
            .args(["worktree", "add", "--detach", "-q"])
            .arg(&wt_path)
            .arg("HEAD")
            .current_dir(project_root)
            .status()?;
        anyhow::ensure!(status.success(), "git worktree add failed");

        for entry in walk_rs_files(&wt_path.join("src"))? {
            let source = std::fs::read_to_string(&entry)?;
            std::fs::write(&entry, rewrite_pub_to_pub_crate(&source))?;
        }

        Ok(Self {
            root: wt_path,
            source_repo: project_root.to_path_buf(),
            _scratch: scratch,
        })
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

fn walk_rs_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
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

        let wt = RewrittenWorktree::create(tmp.path()).unwrap();

        let rewritten = std::fs::read_to_string(wt.root.join("src/lib.rs")).unwrap();
        assert!(rewritten.contains("pub(crate) fn dead"));

        let original = std::fs::read_to_string(tmp.path().join("src/lib.rs")).unwrap();
        assert_eq!(
            original, "pub fn dead() {}\n",
            "the caller's real tree must be untouched"
        );
    }

    #[test]
    fn the_worktree_directory_is_removed_when_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        init_fixture_repo(tmp.path());

        let wt_root = {
            let wt = RewrittenWorktree::create(tmp.path()).unwrap();
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
            let _wt = RewrittenWorktree::create(tmp.path()).unwrap();
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
