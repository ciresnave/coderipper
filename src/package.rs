//! Where a project's package lives: inside its git repository, and inside its cargo workspace.
//!
//! A project is the directory the user passes (`--project`). It is usually a whole repository with
//! one package at its root, but it may be a workspace MEMBER (`fuel/fuel-core`) or the root package of
//! a workspace. Checks analyze exactly one package: they rewrite and read its `src/`, build only it
//! (`cargo build` run from its directory builds the package and its dependencies, not its siblings),
//! and read `.coderipper.toml` from its directory.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `dir`'s path inside its git repository: `""` at the repository root, `"fuel-core/"` below it
/// (forward slashes, trailing slash), as `git rev-parse --show-prefix` prints it.
pub(crate) fn git_prefix(dir: &Path) -> anyhow::Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-prefix"])
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{dir:?} is not inside a git repository: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/"))
}

#[derive(Debug, Clone)]
pub(crate) struct Package {
    pub name: String,
    pub dir: PathBuf,
}

pub(crate) struct Metadata {
    pub workspace_root: PathBuf,
    /// Every workspace member (`cargo metadata --no-deps`).
    pub packages: Vec<Package>,
}

pub(crate) fn metadata(dir: &Path) -> anyhow::Result<Metadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(dir)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "cargo metadata failed in {dir:?}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages = json["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(Package {
                name: p["name"].as_str()?.to_string(),
                dir: Path::new(p["manifest_path"].as_str()?)
                    .parent()?
                    .to_path_buf(),
            })
        })
        .collect();
    Ok(Metadata {
        workspace_root: PathBuf::from(json["workspace_root"].as_str().unwrap_or_default()),
        packages,
    })
}

fn same_dir(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// The package whose own directory is `dir`. A virtual workspace root, or any directory that is not
/// a package's, is an error that names the members to choose from.
pub(crate) fn require_package(dir: &Path) -> anyhow::Result<Package> {
    let meta = metadata(dir)?;
    if let Some(package) = meta.packages.iter().find(|p| same_dir(&p.dir, dir)) {
        return Ok(package.clone());
    }
    let members: Vec<&str> = meta.packages.iter().map(|p| p.name.as_str()).collect();
    anyhow::bail!(
        "{dir:?} is not a package directory (a virtual workspace root, or a directory inside one); \
         pass --project <member directory>. Workspace members: {}",
        members.join(", ")
    )
}

/// `dir` relative to its cargo workspace root: `""` when it is the root, `"b/"` for a member.
/// cargo reports diagnostic file names relative to the workspace root, so this is the prefix to strip.
pub(crate) fn workspace_prefix(dir: &Path) -> anyhow::Result<String> {
    let meta = metadata(dir)?;
    let root = meta.workspace_root.canonicalize()?;
    let relative = dir.canonicalize()?;
    let relative = relative.strip_prefix(&root)?;
    let text = relative.to_string_lossy().replace('\\', "/");
    Ok(if text.is_empty() {
        text
    } else {
        format!("{text}/")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (name, contents) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
    }

    fn pkg(name: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
    }

    /// A virtual workspace with members `a` and `b`, as its own git repository.
    fn virtual_workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            &[
                (
                    "Cargo.toml",
                    "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
                ),
                ("a/Cargo.toml", &pkg("a")),
                ("a/src/lib.rs", "pub fn f() {}\n"),
                ("b/Cargo.toml", &pkg("b")),
                ("b/src/lib.rs", "pub fn g() {}\n"),
            ],
        );
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
                "i",
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
    fn the_git_prefix_is_empty_at_the_root_and_the_relative_path_below_it() {
        let ws = virtual_workspace();
        assert_eq!(git_prefix(ws.path()).unwrap(), "");
        assert_eq!(git_prefix(&ws.path().join("a")).unwrap(), "a/");
    }

    #[test]
    fn a_directory_outside_any_git_repository_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(git_prefix(tmp.path()).is_err());
    }

    #[test]
    fn a_member_directory_is_a_package_and_its_workspace_prefix_is_its_path() {
        let ws = virtual_workspace();
        let b = ws.path().join("b");
        assert_eq!(require_package(&b).unwrap().name, "b");
        assert_eq!(workspace_prefix(&b).unwrap(), "b/");
    }

    #[test]
    fn a_virtual_workspace_root_is_refused_with_the_member_names() {
        let ws = virtual_workspace();
        let err = require_package(ws.path()).unwrap_err().to_string();
        assert!(err.contains("not a package directory"), "{err}");
        assert!(err.contains("a, b") || err.contains("b, a"), "{err}");
    }

    #[test]
    fn a_workspace_root_that_is_itself_a_package_is_a_package() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            &[
                (
                    "Cargo.toml",
                    &format!("{}\n[workspace]\nmembers = [\"sub\"]\n", pkg("root")),
                ),
                ("src/lib.rs", ""),
                ("sub/Cargo.toml", &pkg("sub")),
                ("sub/src/lib.rs", ""),
            ],
        );
        assert_eq!(require_package(tmp.path()).unwrap().name, "root");
        assert_eq!(workspace_prefix(tmp.path()).unwrap(), "");
        assert_eq!(workspace_prefix(&tmp.path().join("sub")).unwrap(), "sub/");
    }
}
