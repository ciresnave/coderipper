//! The little of GitHub this tool needs: read one JSON document from the REST API, and work out which
//! repository a git checkout belongs to.
//!
//! The client is `gh api <path>` — the portfolio's own authenticated GitHub client — behind the
//! [`Github`] trait so a check's logic is tested with canned responses (copied from real ones) and no
//! network. This module never switches `gh` accounts and never writes to GitHub: it only GETs.

use std::path::Path;
use std::process::Command;

/// A failed API call. `status` is the HTTP status when GitHub answered, `None` when it never did
/// (no `gh`, no network, not authenticated).
///
/// Build one (for a fake [`Github`]) with [`ApiError::new`]: the struct is `#[non_exhaustive]`, so a struct literal is
/// rejected outside this crate.
///
/// ```compile_fail,E0639
/// use coderipper::github::ApiError;
/// let _ = ApiError { status: Some(404), message: "not found".to_string() };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApiError {
    /// The HTTP status, when GitHub answered.
    pub status: Option<u16>,
    /// What went wrong, in words.
    pub message: String,
}

impl ApiError {
    /// A failed call: `status` is the HTTP status when GitHub answered, `None` when it never did.
    pub fn new(status: Option<u16>, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            Some(status) => write!(f, "{} (HTTP {status})", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ApiError {}

/// Reads one JSON document from the GitHub REST API. `path` is relative to the API root, e.g.
/// `repos/ciresnave/coderipper/branches/main`.
pub trait Github {
    /// Reads `path` (relative to the API root) and returns the JSON document, or why it could not.
    fn get(&self, path: &str) -> Result<serde_json::Value, ApiError>;
}

/// `gh api` arguments. Pinned to github.com so a `GH_HOST` in the environment cannot send the question to
/// another server whose repository happens to share the name.
pub(crate) fn gh_args(path: &str) -> Vec<String> {
    ["api", "--hostname", "github.com", path]
        .map(String::from)
        .to_vec()
}

/// The real client: `gh api <path>`, as whichever account `gh` has active.
pub struct GhCli;

impl Github for GhCli {
    fn get(&self, path: &str) -> Result<serde_json::Value, ApiError> {
        let output = Command::new("gh")
            .args(gh_args(path))
            .output()
            .map_err(|e| ApiError {
                status: None,
                message: format!(
                    "cannot run `gh` ({e}); install the GitHub CLI and `gh auth login`"
                ),
            })?;
        if output.status.success() {
            serde_json::from_slice(&output.stdout).map_err(|e| ApiError {
                status: None,
                message: format!("GitHub's answer for {path} was not JSON: {e}"),
            })
        } else {
            Err(parse_gh_failure(&output.stdout, &output.stderr))
        }
    }
}

/// `gh api` prints the API's JSON error body on stdout (`{"message":"Not Found","status":"404"}`) and
/// `gh: Not Found (HTTP 404)` on stderr. Take the status and message from whichever is there.
pub(crate) fn parse_gh_failure(stdout: &[u8], stderr: &[u8]) -> ApiError {
    let body: Option<serde_json::Value> = serde_json::from_slice(stdout).ok();
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    let status = body
        .as_ref()
        .and_then(|b| b.get("status"))
        .and_then(|s| {
            s.as_u64()
                .or_else(|| s.as_str().and_then(|text| text.parse().ok()))
        })
        .or_else(|| {
            stderr
                .split("(HTTP ")
                .nth(1)
                .and_then(|rest| rest.split(')').next())
                .and_then(|n| n.trim().parse().ok())
        })
        .map(|n| n as u16);
    let message = body
        .as_ref()
        .and_then(|b| b.get("message"))
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .filter(|m| !m.is_empty())
        .unwrap_or(stderr);
    ApiError { status, message }
}

/// Which GitHub repository a remote URL names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    /// The user or organisation.
    pub owner: String,
    /// The repository's name, without `.git`.
    pub name: String,
}

/// `https://github.com/o/n(.git)`, `git@github.com:o/n(.git)`, `ssh://git@github.com/o/n(.git)`,
/// `git://github.com/o/n`, with or without credentials, a trailing slash, or `.git`. Any other host
/// (GitLab, a local path) is `None`.
pub fn parse_github_remote(url: &str) -> Option<RepoRef> {
    let url = url.trim();
    let path = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        host.eq_ignore_ascii_case("github.com").then_some(path)?
    } else {
        let rest = url.split_once("://")?.1;
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        let host = host.split(':').next()?;
        host.eq_ignore_ascii_case("github.com").then_some(path)?
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let valid = |s: &str| !s.is_empty() && !s.contains('/');
    (valid(owner) && valid(name)).then(|| RepoRef {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// A `git` command for the repository at `dir`. An inherited `GIT_DIR` / `GIT_WORK_TREE` (git exports them
/// to hooks) would override `current_dir` and make the question about ANOTHER repository.
pub(crate) fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE");
    cmd
}

/// The URL of `origin` in the git repository containing `dir`.
pub fn origin_url(dir: &Path) -> anyhow::Result<String> {
    let output = git_command(dir, &["remote", "get-url", "origin"]).output()?;
    anyhow::ensure!(
        output.status.success(),
        "{} has no `origin` remote ({}); ci-protection-presence needs to know which GitHub repository it is",
        dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, name: &str) -> Option<RepoRef> {
        Some(RepoRef {
            owner: owner.into(),
            name: name.into(),
        })
    }

    #[test]
    fn every_common_github_remote_form_is_understood() {
        for url in [
            "https://github.com/ciresnave/coderipper.git",
            "https://github.com/ciresnave/coderipper",
            "https://github.com/ciresnave/coderipper/",
            "http://github.com/ciresnave/coderipper.git",
            "https://user:token@github.com/ciresnave/coderipper.git",
            "git@github.com:ciresnave/coderipper.git",
            "git@github.com:ciresnave/coderipper",
            "ssh://git@github.com/ciresnave/coderipper.git",
            "ssh://git@github.com:22/ciresnave/coderipper.git",
            "git://github.com/ciresnave/coderipper.git",
            "https://GitHub.com/ciresnave/coderipper.git",
        ] {
            assert_eq!(
                parse_github_remote(url),
                repo("ciresnave", "coderipper"),
                "{url}"
            );
        }
    }

    #[test]
    fn other_hosts_and_malformed_urls_are_not_github() {
        for url in [
            "https://gitlab.com/ciresnave/coderipper.git",
            "git@gitlab.com:ciresnave/coderipper.git",
            "https://github.com.evil.example/ciresnave/coderipper.git",
            "https://github.com/ciresnave",
            "https://github.com/a/b/c",
            "/home/me/repos/coderipper",
            "C:\\repos\\coderipper",
            "",
        ] {
            assert_eq!(parse_github_remote(url), None, "{url}");
        }
    }

    #[test]
    fn a_gh_failure_yields_the_status_and_message_from_the_json_body() {
        let body = br#"{"message":"Branch not found","documentation_url":"https://docs.github.com/x","status":"404"}"#;
        let err = parse_gh_failure(body, b"gh: Branch not found (HTTP 404)");
        assert_eq!(err.status, Some(404));
        assert_eq!(err.message, "Branch not found");
    }

    #[test]
    fn a_gh_failure_without_a_body_falls_back_to_stderr() {
        let err = parse_gh_failure(b"", b"gh: Bad credentials (HTTP 401)\n");
        assert_eq!(err.status, Some(401));
        assert!(err.message.contains("Bad credentials"), "{}", err.message);
        let err = parse_gh_failure(b"", b"gh: To use GitHub CLI, run: gh auth login");
        assert_eq!(err.status, None);
        assert!(err.message.contains("gh auth login"));
    }

    #[test]
    fn git_is_run_without_an_inherited_git_dir() {
        // Review finding: an exported GIT_DIR (git sets it for hooks) made `git remote get-url origin`
        // read ANOTHER repository's origin and judge that one.
        let cmd = git_command(std::path::Path::new("."), &["remote", "get-url", "origin"]);
        let removed: Vec<_> = cmd
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().to_string())
            .collect();
        assert!(removed.contains(&"GIT_DIR".to_string()), "{removed:?}");
        assert!(
            removed.contains(&"GIT_WORK_TREE".to_string()),
            "{removed:?}"
        );
    }

    #[test]
    fn gh_is_pinned_to_github_dot_com_whatever_gh_host_says() {
        assert_eq!(
            gh_args("repos/a/b"),
            vec!["api", "--hostname", "github.com", "repos/a/b"]
        );
    }

    #[test]
    fn the_real_empty_repository_answer_is_a_409_with_its_message() {
        // Copied from `gh api repos/ciresnave/bayes-optimal/commits?per_page=1` (2026-10-02): the status is a
        // STRING in the body, and gh's stderr repeats it.
        let err = parse_gh_failure(
            br#"{"message":"Git Repository is empty.","documentation_url":"https://docs.github.com/rest/commits/commits#list-commits","status":"409"}"#,
            b"gh: Git Repository is empty. (HTTP 409)
",
        );
        assert_eq!(err.status, Some(409));
        assert!(err.message.contains("empty"), "{}", err.message);
    }

    #[test]
    fn a_numeric_status_in_the_body_is_accepted_too() {
        let err = parse_gh_failure(br#"{"message":"Forbidden","status":403}"#, b"");
        assert_eq!(err.status, Some(403));
    }

    #[test]
    fn the_origin_of_a_repository_is_read_and_a_missing_origin_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert!(origin_url(tmp.path()).is_err(), "no origin yet");
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widgets.git",
            ])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert_eq!(
            origin_url(tmp.path()).unwrap(),
            "https://github.com/acme/widgets.git"
        );
    }
}
