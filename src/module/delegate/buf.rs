//! API-006 (schema changes obey evolution rules), delegated to [buf](https://github.com/bufbuild/buf) (Apache-2.0): `buf breaking`
//! over the project's Protocol Buffers schemas.
//!
//! **What this judges:** the *working tree's* `.proto` files against a **baseline**: the schema as a git commit has it. buf's `FILE`
//! category is used, the strictest and buf's own default (retyping or renumbering a field, removing a field, message, enum value or
//! file, moving a message to another package, ...). The baseline is, in order: the `[buf] baseline = "<ref>"` of the project's
//! `.coderipper.toml` (a branch, tag or commit: normally the last release), else the merge-base of `HEAD` with `origin/HEAD`
//! (`origin/main`, `origin/master`), the point the branch left the default branch at.
//!
//! **Nothing compared is a gap, never clean:** `buf breaking` compares two versions, and asking it to compare a tree with itself, or
//! to look at schemas that did not change, gives an empty answer that is not a check. So the rule is a coverage gap when the baseline
//! cannot be found (no `origin`, a shallow clone without the merge-base); when **no `.proto` file differs** from it (a run on the
//! default branch: the merge-base of `HEAD` with itself is `HEAD`, and a clean tree is the baseline; uncommitted schema changes there
//! are compared with `HEAD`); when the files that differ are **not below a module that has schema on both sides** (buf is not given
//! them: measured, it exits 0 and prints nothing); when the baseline holds no `.proto` file below a module (a schema that is new cannot
//! break anyone); and when every schema was removed from the tree (buf needs a module on both sides). A module that had schema in the
//! baseline and has none now cannot be compared either (buf refuses a module with no schema, and needs the same modules on both sides:
//! measured), although removing a schema is a breaking change: the result carries a note, and a clean result becomes a gap.
//! A baseline that was *named* and does not resolve is an error: the project asked for something that is not there.
//!
//! **Where the schemas are:** buf wants the root a `.proto` file's imports are relative to. The roots are read from the `buf.yaml` /
//! `buf.work.yaml` files of each side (the tree's from disk, the baseline's from git; v1: the file's directory; v2: the directory,
//! or the `path:` of each of its `modules:`, also in flow style; a work file: its `directories:`), else the project's own directory.
//! Nothing else of those files is used. The modules compared are those with a schema file on both sides. buf cannot compile a file with an
//! import it cannot find or a syntax error, and builds all of the schemas or none (measured: with one file that does not compile it
//! prints that error and **no** violation at all), so the rule is then a **coverage gap** naming the file, never clean.
//!
//! **The repository cannot silence the rule:** buf reads its configuration from a `buf.yaml` found in the tree and from the baseline
//! (`breaking: ignore:`, `ignore_only:`, `use:`, `except:`). Both are replaced by a configuration given on the command line
//! (`--config` and `--against-config`; measured with buf 1.73.0: a `buf.yaml` that ignores the whole tree changes nothing, nested ones
//! included; no test tells whether `--against-config` is needed, so it is defensive). An inline `// buf:breaking:ignore` comment is
//! not a feature of buf 1.73.0 (measured: the finding comes back), so nothing turns it off. The project's way to suppress a finding is
//! the `.coderipper.toml` allowlist, which is visible.
//!
//! **Only the project's files:** buf walks the module roots on disk, so a finding is kept only for a file the index tracks or the
//! baseline had (a stray `.proto` in an ignored directory is not the project's; a violation left out is counted and the result says so,
//! and a clean result then becomes a gap). A stray schema that conflicts with the project's (the same message declared twice) stops buf
//! from building anything, and the rule is a gap too. The tool runs in an empty scratch directory with its environment reduced to a short
//! allowlist (`PATH`, the temporary and home directories; no `BUF_TOKEN`), and a project with a dependency on the Buf Schema Registry
//! has none of it: an import of a dependency is a file buf cannot compile, so the result is a gap.
//!
//! **The mapping record:** one finding per buf violation, at the file and line it printed, with buf's rule id as the subject and
//! its message as the summary. Severity and confidence are those of the catalog (a breaking schema change is high and buf's answer
//! is exact). buf's text is third-party text that reaches a terminal: control characters are removed and the length is cut.

use super::osv::{clean, relative, roots_of};
use super::{resolve, run_tool, tracked_listing, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The catalog rule this evidences.
pub(super) const RULE: &str = "API-006";
/// The tool's name in the lock.
pub(super) const TOOL: &str = "buf";

/// What `buf` exits with when it found violations (`1` is a failure of the tool).
const EXIT_VIOLATIONS: i32 = 100;
/// What `buf` exits with when it could not do what it was asked.
const EXIT_FAILURE: i32 = 1;
/// Directories of other people's files, which are not the project's to fix.
const SKIPPED_DIRS: &[&str] = &["node_modules", "vendor", "third_party", "target", ".git"];
/// The refs tried, in order, for the default branch's remote copy.
const DEFAULT_BRANCHES: &[&str] = &["origin/HEAD", "origin/main", "origin/master"];

/// `git` in `dir`, with the variables that aim it at another repository cleared; stdout on success.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Whether the project-relative path (with `/`) is below a directory of other people's files.
fn skipped(path: &str) -> bool {
    path.split('/')
        .any(|s| SKIPPED_DIRS.iter().any(|d| s.eq_ignore_ascii_case(d)))
}

/// Whether `path` is a schema file of the project: a `.proto` below `prefix`. (Not filtered by [`skipped`]: a package of the
/// project's own may be called `target` or `vendor`, and a schema the index tracks is the project's.)
fn is_proto(path: &str, prefix: &str) -> bool {
    path.starts_with(prefix) && path.ends_with(".proto")
}

/// Whether `file` is below the module root `root` (`""` is the top of the tree).
fn under(root: &str, file: &str) -> bool {
    root.is_empty()
        || file
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Where the project sits in its repository.
#[derive(Debug, Clone)]
struct Repo {
    /// The top of the working tree (buf is given this as its input: its module paths are relative to it).
    top: PathBuf,
    /// The repository's common git directory (the one that holds the objects, also from a linked worktree).
    common: PathBuf,
    /// The project's directory below `top`, `""` or `dir/` with a trailing slash.
    prefix: String,
    head: String,
}

fn locate(root: &Path) -> Result<Repo, String> {
    let text = git(
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
            "--show-prefix",
        ],
    )?;
    let mut lines = text.split('\n');
    let top = lines.next().unwrap_or_default().trim();
    let common = lines.next().unwrap_or_default().trim();
    let prefix = lines.next().unwrap_or_default().trim();
    if top.is_empty() || common.is_empty() {
        return Err("git did not say where the repository is".into());
    }
    let head = git(root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .map_err(|_| "the repository has no commit yet".to_string())?;
    Ok(Repo {
        top: PathBuf::from(top),
        common: PathBuf::from(common),
        prefix: prefix.to_string(),
        head: head.trim().to_string(),
    })
}

/// The commit a ref names, or why it names none. The ref is not an option: one that starts with `-` is refused.
fn commit_of(root: &Path, reference: &str) -> Result<String, String> {
    if reference.is_empty() || reference.starts_with('-') || reference.contains('\0') {
        return Err(format!("{reference:?} is not a git ref"));
    }
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .map(|s| s.trim().to_string())
    .map_err(|_| format!("{reference:?} is not a commit in this repository"))
}

/// What the baseline came to.
#[derive(Debug, Clone, PartialEq)]
enum Baseline {
    /// A commit, and how it was chosen (for the message).
    Commit { sha: String, how: String },
    /// There is none, in a way that is a coverage gap.
    Gap(String),
    /// A ref the project named does not resolve.
    Bad(String),
}

/// The baseline: the project's named ref, else where the branch left the default branch.
fn baseline(root: &Path, configured: Option<&str>) -> Baseline {
    if let Some(reference) = configured {
        return match commit_of(root, reference) {
            Ok(sha) => Baseline::Commit {
                sha,
                how: format!("the ref `{}` named by [buf] baseline", clean(reference, 80)),
            },
            Err(why) => Baseline::Bad(format!(
                "the baseline named in .coderipper.toml ([buf] baseline) does not resolve: {why}"
            )),
        };
    }
    for candidate in DEFAULT_BRANCHES {
        if commit_of(root, candidate).is_err() {
            continue;
        }
        return match git(root, &["merge-base", "HEAD", candidate]) {
            Ok(sha) => Baseline::Commit {
                sha: sha.trim().to_string(),
                how: format!("the merge-base of HEAD with {candidate}"),
            },
            Err(_) => Baseline::Gap(format!(
                "HEAD and {candidate} share no commit here (a shallow clone does not have the history), so there is no baseline to compare \
                 with; fetch more history or name a ref with `[buf] baseline` in .coderipper.toml"
            )),
        };
    }
    Baseline::Gap(
        "the repository has no origin/HEAD, origin/main or origin/master to take a baseline from; name a ref with `[buf] baseline` in \
         .coderipper.toml"
            .into(),
    )
}

/// The message of the gap that says the tree has nothing distinct to be compared with.
fn not_distinct(how: &str) -> String {
    format!(
        "there is no baseline distinct from the tree: it is compared with {how}, and no .proto file differs from it, so `buf breaking` \
         would compare the schemas with themselves and say nothing; run it on a branch that changes a schema, or set a ref with `[buf] \
         baseline` in .coderipper.toml"
    )
}

/// The module roots (relative to the top of the working tree, with `/`) the project's schemas are read from.
///
/// `listing` is the tracked files, `read` returns the text of a tracked file. Only the tracked `buf.yaml`, `buf.yml` and
/// `buf.work.yaml` files are used, and only for where the roots are.
fn module_roots(listing: &str, prefix: &str, read: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    let join = |dir: &str, rel: &str| -> Option<String> {
        let rel = rel.trim().trim_matches(|c| c == '"' || c == '\'');
        let rel = rel.trim_start_matches("./").trim_end_matches('/');
        if rel.is_empty() || rel == "." {
            return Some(dir.to_string());
        }
        if rel.starts_with('/') || rel.contains(':') || rel.split('/').any(|s| s == "..") {
            return None;
        }
        Some(if dir.is_empty() {
            rel.to_string()
        } else {
            format!("{dir}/{rel}")
        })
    };
    for path in listing.split('\0') {
        if path.is_empty() || !path.starts_with(prefix) || skipped(path) {
            continue;
        }
        let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
        let work = matches!(name, "buf.work.yaml" | "buf.work.yml");
        if !(work || matches!(name, "buf.yaml" | "buf.yml")) {
            continue;
        }
        let Some(text) = read(path) else { continue };
        let mut found: Vec<String> = Vec::new();
        if work {
            // `directories:` and then `- name` lines
            let mut inside = false;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    continue;
                }
                if let Some(rest) = trimmed.strip_prefix("directories:") {
                    inside = rest.trim().is_empty();
                } else if inside {
                    match trimmed.strip_prefix("- ") {
                        Some(item) => found.extend(join(dir, item.split('#').next().unwrap_or(""))),
                        None => inside = false,
                    }
                }
            }
        } else {
            // v2 lists its modules by `path:`; v1, and v2 with no `modules:`, are the directory itself
            let modules = text.lines().any(|l| l.trim_start().starts_with("modules:"));
            if modules {
                for line in text.lines() {
                    let line = line.split('#').next().unwrap_or("");
                    for (at, key) in line.match_indices("path:") {
                        // a key, not the end of another word (`xpath:`)
                        if line[..at]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        {
                            continue;
                        }
                        let value = &line[at + key.len()..];
                        let end = value.find([',', '}', ']']).unwrap_or(value.len());
                        found.extend(join(dir, &value[..end]));
                    }
                }
            } else {
                found.push(dir.to_string());
            }
        }
        roots.extend(found);
    }
    if roots.is_empty() {
        roots.push(prefix.trim_end_matches('/').to_string());
    }
    roots.sort();
    roots.dedup();
    // buf refuses modules that contain one another: the outer one is kept
    let all = roots.clone();
    roots.retain(|r| {
        !all.iter().any(|o| {
            o != r
                && (o.is_empty()
                    || r.strip_prefix(o.as_str())
                        .is_some_and(|t| t.starts_with('/')))
        })
    });
    roots
}

/// The configuration buf is given for both sides of the comparison, in place of every `buf.yaml`.
fn config(roots: &[String]) -> String {
    let modules: Vec<serde_json::Value> = roots
        .iter()
        .map(|r| serde_json::json!({ "path": if r.is_empty() { "." } else { r.as_str() } }))
        .collect();
    serde_json::json!({
        "version": "v2",
        "modules": modules,
        "breaking": { "use": ["FILE"] },
    })
    .to_string()
}

/// Everything decided before buf runs.
#[derive(Debug, Clone)]
struct Plan {
    repo: Repo,
    sha: String,
    how: String,
    /// The module roots present, with schema, both in the tree and in the baseline: buf needs the same modules on both sides.
    roots: Vec<String>,
    /// Module roots that had schema in the baseline and have none now: buf cannot compare them.
    removed: Vec<String>,
    /// The files whose findings are the project's: tracked now, or in the baseline.
    known: HashSet<String>,
}

/// What preparing the run came to.
enum Prepared {
    Ready(Box<Plan>),
    Gap(String),
    Failed(ErrorKind, String),
}

fn prepare(root: &Path, configured: Option<&str>) -> Prepared {
    let repo = match locate(root) {
        Ok(repo) => repo,
        Err(why) => {
            return Prepared::Gap(format!(
                "the project is not in a git repository with a commit ({why}), so there is no baseline"
            ));
        }
    };
    let (sha, how) = match baseline(root, configured) {
        Baseline::Commit { sha, how } => (sha, how),
        Baseline::Gap(why) => return Prepared::Gap(why),
        Baseline::Bad(why) => return Prepared::Failed(ErrorKind::ConfigInvalid, why),
    };
    // On the default branch the merge-base of HEAD with origin/HEAD is HEAD: the baseline is then the tree's own commit, and it is a
    // comparison only if the working tree has changed a schema since (below); a clean tree is compared with itself.
    let how = if sha == repo.head {
        format!("{how}, which is HEAD")
    } else {
        how
    };
    let listing = match tracked_listing(&repo.top) {
        Ok(listing) => listing,
        Err(why) => return Prepared::Failed(ErrorKind::ToolFailed, why),
    };
    // `--no-renames`: a schema file renamed away from `.proto` is a deletion (and the new name an addition), not one entry
    let changed = match git(
        &repo.top,
        &["diff", "--name-only", "--no-renames", "-z", &sha, "--"],
    ) {
        Ok(text) => text,
        Err(why) => return Prepared::Failed(ErrorKind::ToolFailed, why),
    };
    let changed: Vec<&str> = changed
        .split('\0')
        .filter(|p| is_proto(p, &repo.prefix))
        .collect();
    if changed.is_empty() {
        return Prepared::Gap(not_distinct(&format!("{how} ({})", short(&sha))));
    }
    let then = match git(&repo.top, &["ls-tree", "-r", "-z", "--name-only", &sha]) {
        Ok(text) => text,
        Err(why) => return Prepared::Failed(ErrorKind::ToolFailed, why),
    };
    let protos = |listing: &str| -> HashSet<String> {
        listing
            .split('\0')
            .filter(|p| is_proto(p, &repo.prefix))
            .map(str::to_string)
            .collect()
    };
    let (tree_files, base_files) = (protos(&listing), protos(&then));
    let known: HashSet<String> = tree_files.union(&base_files).cloned().collect();
    // each side's module roots are read from its own buf.yaml / buf.work.yaml files (the tree's from disk, the baseline's from git)
    let top = repo.top.clone();
    let read_tree = move |p: &str| std::fs::read_to_string(top.join(p)).ok();
    let (dir, at) = (repo.top.clone(), sha.clone());
    let read_base = move |p: &str| git(&dir, &["show", &format!("{at}:{p}")]).ok();
    let tree_roots = roots_with(&repo.prefix, &tree_files, &listing, &read_tree);
    let base_roots = roots_with(&repo.prefix, &base_files, &then, &read_base);
    if base_roots.is_empty() {
        return Prepared::Gap(format!(
            "the baseline ({how}, {}) holds no .proto file below a buf module, so there is no earlier schema to break (a schema that is new cannot break anyone)",
            short(&sha)
        ));
    }
    if tree_roots.is_empty() {
        return Prepared::Gap(format!(
            "no tracked .proto file is left below a buf module, while the baseline ({how}, {}) had some: every schema was removed, which buf cannot compare (it needs a module on both sides)",
            short(&sha)
        ));
    }
    // buf compares the same modules on both sides (measured: "input contained 1 images, whereas against contained 2"), and a module
    // with no schema on one side is refused
    let roots: Vec<String> = tree_roots
        .iter()
        .filter(|r| base_roots.contains(r))
        .cloned()
        .collect();
    let removed: Vec<String> = base_roots
        .iter()
        .filter(|r| !tree_roots.contains(r))
        .cloned()
        .collect();
    if roots.is_empty() {
        return Prepared::Gap(format!(
            "no buf module has schema both in the tree and in the baseline ({how}, {}) (tree: {}; baseline: {}), so there is nothing buf can compare",
            short(&sha),
            root_list(&tree_roots),
            root_list(&base_roots)
        ));
    }
    // a changed schema file buf is not given (outside every module, or in a module that is gone) is not compared: buf would say
    // nothing about it
    if removed.is_empty() && !changed.iter().any(|p| roots.iter().any(|r| under(r, p))) {
        return Prepared::Gap(format!(
            "the .proto files that differ from {how} ({}) are not below a buf module that has schema both in the tree and in the baseline ({}), so `buf breaking` would compare nothing that changed",
            short(&sha),
            root_list(&roots)
        ));
    }
    for text in [
        repo.top.display().to_string(),
        repo.common.display().to_string(),
    ] {
        if text.contains('#') || text.contains(',') {
            return Prepared::Gap(format!(
                "the repository path contains a `#` or a `,`, which buf reads as part of its input syntax, so it cannot be given to buf ({})",
                clean(&text, 120)
            ));
        }
    }
    Prepared::Ready(Box::new(Plan {
        repo,
        sha,
        how,
        roots,
        removed,
        known,
    }))
}

/// The module roots of one side: those the side's buf files name that have a schema file of that side below them.
fn roots_with(
    prefix: &str,
    files: &HashSet<String>,
    listing: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    module_roots(listing, prefix, read)
        .into_iter()
        .filter(|r| files.iter().any(|p| under(r, p)))
        .collect()
}

/// Module roots for a message (`.` is the top of the tree).
fn root_list(roots: &[String]) -> String {
    let shown: Vec<String> = roots
        .iter()
        .map(|r| {
            if r.is_empty() {
                ".".to_string()
            } else {
                clean(r, 60)
            }
        })
        .collect();
    if shown.is_empty() {
        "none".into()
    } else {
        shown.join(", ")
    }
}

fn short(sha: &str) -> &str {
    sha.get(..10).unwrap_or(sha)
}

/// The arguments of the comparison: buf is given the top of the working tree, the baseline as a commit of the repository, and the
/// same configuration for both, with JSON on stdout.
fn check_args(plan: &Plan) -> Vec<OsString> {
    let config = config(&plan.roots);
    let against = format!("{}#ref={}", plan.repo.common.display(), plan.sha);
    vec![
        OsString::from("breaking"),
        plan.repo.top.clone().into_os_string(),
        OsString::from("--against"),
        OsString::from(against),
        OsString::from("--config"),
        OsString::from(config.clone()),
        OsString::from("--against-config"),
        OsString::from(config),
        OsString::from("--error-format"),
        OsString::from("json"),
    ]
}

#[derive(Debug, Deserialize)]
struct Violation {
    /// Absent for a violation about a whole file that is no longer there (`FILE_NO_DELETE`): the message names it.
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    start_line: u32,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    message: String,
}

/// One breaking change, with the file as the project names it.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    rule: String,
    file: String,
    line: u32,
    message: String,
}

/// What buf printed.
#[derive(Debug, Default)]
struct Report {
    hits: Vec<Hit>,
    /// The first thing buf said it could not compile.
    unread: Option<String>,
    /// How many violations were about a file that is neither tracked nor in the baseline, and were left out.
    dropped: usize,
}

/// Reads buf's `--error-format json` output (one object per line). A line that is not one is unreadable; a `COMPILE` entry is a
/// file buf could not build, which is not a finding. `roots` are the names the top of the tree can be printed as; `prefix` is the
/// project's directory below it, taken off the path.
fn parse_report(
    text: &str,
    roots: &[String],
    prefix: &str,
    known: &HashSet<String>,
) -> Result<Report, String> {
    let mut report = Report::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let v: Violation = serde_json::from_str(line).map_err(|e| {
            format!(
                "an output line is not a buf violation ({e}): {}",
                line.chars().take(200).collect::<String>()
            )
        })?;
        let from_top = match &v.path {
            Some(path) => relative(path, roots),
            None => deleted_file(&v.message, known).unwrap_or_default(),
        };
        if v.kind == "COMPILE" {
            report.unread.get_or_insert_with(|| {
                format!("{}: {}", clean(&from_top, 120), clean(&v.message, 120))
            });
            continue;
        }
        // a violation with no file at all has no place to be named; it is kept, without a location
        if !(from_top.is_empty() && v.path.is_none()) && !known.contains(&from_top) {
            report.dropped += 1;
            continue;
        }
        report.hits.push(Hit {
            rule: v.kind,
            file: from_top
                .strip_prefix(prefix)
                .unwrap_or(&from_top)
                .to_string(),
            line: v.start_line.max(1),
            message: v.message,
        });
    }
    report
        .hits
        .sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    Ok(report)
}

/// The file a path-less violation is about. buf names it in the message, relative to its module (`Previously present file
/// "a/v1/gone.proto" was deleted.`); it is taken only if it is exactly one file the baseline or the index has.
fn deleted_file(message: &str, known: &HashSet<String>) -> Option<String> {
    let name = message.split('"').nth(1)?;
    if !name.ends_with(".proto") {
        return None;
    }
    let tail = format!("/{name}");
    let mut found = known
        .iter()
        .filter(|p| p.as_str() == name || p.ends_with(&tail));
    let first = found.next()?;
    found.next().is_none().then(|| first.clone())
}

/// What the whole project came to.
#[derive(Debug, Clone)]
enum Scan {
    Read {
        hits: Vec<Hit>,
        /// What was not judged, in words (each is a reason a clean result cannot be believed).
        notes: Vec<String>,
        how: String,
    },
    Gap(String),
    Failed(ErrorKind, String),
}

fn scan_with(
    program: &Path,
    args: fn(&Plan) -> Vec<OsString>,
    request: &Request,
    configured: Option<&str>,
) -> Scan {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and check nothing.
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    if !root.is_dir() {
        return Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{} is not a directory", root.display()),
        );
    }
    let plan = match prepare(&root, configured) {
        Prepared::Ready(plan) => plan,
        Prepared::Gap(why) => return Scan::Gap(why),
        Prepared::Failed(kind, why) => return Scan::Failed(kind, why),
    };
    let how = format!("{} ({})", plan.how, short(&plan.sha));
    let run = match run_tool(
        program,
        |_scratch| args(&plan),
        &request.limits,
        &[EXIT_VIOLATIONS, EXIT_FAILURE],
    ) {
        Ok(run) => run,
        Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
    };
    match run.exit_code() {
        Some(0) | Some(EXIT_VIOLATIONS) => {}
        other => {
            return Scan::Failed(
                ErrorKind::ToolFailed,
                format!(
                    "{TOOL} exited with {other:?}: {}",
                    clean(&format!("{} {}", run.stdout(), run.log()), 400)
                ),
            );
        }
    }
    let names = roots_of(&plan.repo.top);
    let report = match parse_report(&run.stdout(), &names, &plan.repo.prefix, &plan.known) {
        Ok(report) => report,
        Err(why) => return Scan::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}")),
    };
    if run.exit_code() == Some(0) && !run.stdout().trim().is_empty() {
        return Scan::Failed(
            ErrorKind::ToolOutputUnreadable,
            format!(
                "{TOOL} exited 0 but printed output: {}",
                clean(&run.stdout(), 200)
            ),
        );
    }
    if report.hits.len() > request.limits.max_findings {
        return Scan::Failed(
            ErrorKind::LimitExceeded,
            format!(
                "{TOOL} reported {} violations, past the limit of {}",
                report.hits.len(),
                request.limits.max_findings
            ),
        );
    }
    let mut notes = Vec::new();
    if let Some(why) = &report.unread {
        notes.push(format!(
            "{TOOL} could not compile part of the project ({}); what it could not build was not judged.",
            clean(why, 240)
        ));
    }
    if report.dropped > 0 {
        notes.push(format!(
            "{} violation(s) {TOOL} reported are in files that are neither tracked nor in the baseline and were left out.",
            report.dropped
        ));
    }
    if !plan.removed.is_empty() {
        notes.push(format!(
            "the module(s) {} had schema in the baseline and have none now; buf cannot compare a module that is gone (removing a schema file is a breaking change), so that was not judged.",
            root_list(&plan.removed)
        ));
    }
    Scan::Read {
        hits: report.hits,
        notes,
        how,
    }
}

/// What a result says about what was not judged.
fn caveat(notes: &[String]) -> Option<String> {
    (!notes.is_empty()).then(|| format!("Note: {}", notes.join(" ")))
}

fn to_finding(hit: &Hit, project: &str, how: &str, extra: Option<&str>) -> Finding {
    let file = clean(&hit.file, 200);
    let message = clean(&hit.message, 240);
    let rule = clean(&hit.rule, 60);
    let place = if file.is_empty() {
        "a change to the schema as a whole".to_string()
    } else {
        format!("{file} line {}", hit.line)
    };
    let mut detail = format!(
        "buf ({rule}) reports {place}: {message}. It compares the working tree with {how}. A schema that old readers and writers \
         still use cannot change this way: add a new field with a new number (and `reserved` the old one) instead, or version the \
         message."
    );
    if let Some(extra) = extra {
        detail.push(' ');
        detail.push_str(extra);
    }
    let mut finding = Finding::new(
        RULE,
        Severity::High,
        Confidence::High,
        project,
        format!("{message} ({rule})"),
        detail,
    )
    .subject(rule.clone());
    if !file.is_empty() {
        finding = finding.location(Location::new(file.clone(), Some(hit.line)));
    }
    // buf's message quotes the schema's own names and could contain a whole word the host reads as an absence claim: such a
    // finding carries a control
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "buf read {file} and reported {rule} at line {}",
            hit.line
        )),
    }
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    let program = match resolve(env, TOOL) {
        Resolved::Ready(program) => program,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"));
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    let allowlist = match crate::allowlist::Allowlist::load(&root) {
        Ok(a) => a,
        Err(e) => {
            return Verdict::Failed(
                ErrorKind::ConfigInvalid,
                format!("cannot read .coderipper.toml: {e}"),
            );
        }
    };
    match scan_with(&program, check_args, request, allowlist.buf_baseline()) {
        Scan::Read { hits, notes, how } => {
            let note = caveat(&notes);
            if hits.is_empty() {
                if let Some(note) = note {
                    return Verdict::Unavailable(format!(
                        "{TOOL} found nothing to report, but it cannot say the schemas are compatible. {note}"
                    ));
                }
                return Verdict::Findings(vec![]);
            }
            let project = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Verdict::Findings(
                hits.iter()
                    .map(|h| to_finding(h, &project, &how, note.as_deref()))
                    .collect(),
            )
        }
        Scan::Gap(detail) => Verdict::Unavailable(detail),
        Scan::Failed(kind, detail) => Verdict::Failed(kind, detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_none(_: &str) -> Option<String> {
        None
    }

    fn files(texts: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = texts
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect();
        move |p: &str| owned.iter().find(|(n, _)| n == p).map(|(_, t)| t.clone())
    }

    #[test]
    fn with_no_buf_yaml_the_root_is_the_project_directory() {
        assert_eq!(module_roots("a/v1/s.proto\0", "", &read_none), [""]);
        assert_eq!(module_roots("svc/a/s.proto\0", "svc/", &read_none), ["svc"]);
    }

    #[test]
    fn a_v1_buf_yaml_makes_its_directory_a_root_and_a_v2_one_its_modules() {
        let listing = "proto/buf.yaml\0proto/a/s.proto\0";
        let read = files(&[(
            "proto/buf.yaml",
            "version: v1\nbreaking:\n  use:\n    - FILE\n",
        )]);
        assert_eq!(module_roots(listing, "", &read), ["proto"]);
        let listing = "buf.yaml\0api/a.proto\0common/b.proto\0";
        let read = files(&[(
            "buf.yaml",
            "version: v2\nmodules:\n  - path: api  # the api\n  - path: \"common/\"\n",
        )]);
        assert_eq!(module_roots(listing, "", &read), ["api", "common"]);
        // a v2 file with no modules is its own directory
        let read = files(&[("buf.yaml", "version: v2\nlint:\n  use:\n    - STANDARD\n")]);
        assert_eq!(module_roots("buf.yaml\0", "", &read), [""]);
    }

    #[test]
    fn a_v2_file_in_flow_style_names_its_modules_too() {
        let read = files(&[(
            "buf.yaml",
            "version: v2\nmodules: [{path: api}, {path: \"common\"}]  # both\nxpath: nope\n",
        )]);
        assert_eq!(module_roots("buf.yaml\0", "", &read), ["api", "common"]);
    }

    #[test]
    fn a_file_is_under_a_root_only_by_whole_directory_names() {
        assert!(under("", "a/b.proto") && under("a", "a/b.proto"));
        assert!(!under("a", "ab/b.proto") && !under("a/b", "a/b.proto"));
    }

    #[test]
    fn a_schema_in_a_directory_called_target_is_the_projects() {
        assert!(is_proto("target/v1/t.proto", "") && is_proto("vendor/x.proto", ""));
        assert!(!is_proto("svc/a.proto", "other/") && !is_proto("a.txt", ""));
    }

    #[test]
    fn a_violation_in_a_file_that_is_not_known_is_counted_not_silently_lost() {
        let report = parse_report(BREAK, &roots(), "svc/", &known(&["svc/a/other.proto"])).unwrap();
        assert!(report.hits.is_empty());
        assert_eq!(report.dropped, 1);
    }

    #[test]
    fn a_work_file_lists_directories_and_a_root_inside_another_is_dropped() {
        let listing = "buf.work.yaml\0";
        let read = files(&[(
            "buf.work.yaml",
            "version: v1\ndirectories:\n  - one\n  - two/inner\n  - two\n# - ignored\n",
        )]);
        assert_eq!(module_roots(listing, "", &read), ["one", "two"]);
    }

    #[test]
    fn a_root_that_leaves_the_project_is_not_used() {
        let listing = "buf.work.yaml\0";
        let read = files(&[(
            "buf.work.yaml",
            "directories:\n  - ../outside\n  - /abs\n  - C:/drive\n  - fine\n",
        )]);
        assert_eq!(module_roots(listing, "", &read), ["fine"]);
    }

    #[test]
    fn a_buf_yaml_below_a_skipped_directory_or_outside_the_project_is_not_read() {
        let listing = "node_modules/x/buf.yaml\0other/buf.yaml\0svc/a.proto\0";
        let read = files(&[
            ("node_modules/x/buf.yaml", "version: v1\n"),
            ("other/buf.yaml", "version: v1\n"),
        ]);
        assert_eq!(module_roots(listing, "svc/", &read), ["svc"]);
    }

    #[test]
    fn the_configuration_replaces_every_buf_yaml_and_selects_file() {
        let value: serde_json::Value =
            serde_json::from_str(&config(&["proto".to_string(), String::new()])).unwrap();
        assert_eq!(value["version"], "v2");
        assert_eq!(value["modules"][0]["path"], "proto");
        assert_eq!(value["modules"][1]["path"], ".");
        assert_eq!(value["breaking"]["use"][0], "FILE");
        assert!(value.get("lint").is_none() && value["breaking"].get("ignore").is_none());
    }

    #[test]
    fn a_ref_that_is_an_option_is_refused_before_git_sees_it() {
        let here = std::env::current_dir().unwrap();
        for bad in ["", "-x", "--upload-pack=x"] {
            let why = commit_of(&here, bad).unwrap_err();
            assert!(why.contains("not a git ref"), "{why}");
        }
    }

    fn known(files: &[&str]) -> HashSet<String> {
        files.iter().map(|f| f.to_string()).collect()
    }

    const BREAK: &str = r#"{"path":"C:\\work\\repo\\svc\\a\\s.proto","start_line":7,"start_column":3,"end_line":7,"end_column":9,"type":"FIELD_SAME_TYPE","message":"Field \"2\" with name \"age\" on message \"Foo\" changed type from \"int32\" to \"string\"."}"#;
    const COMPILE: &str = r#"{"path":"svc\\a\\imp.proto","start_line":3,"start_column":1,"end_line":3,"end_column":21,"type":"COMPILE","message":"imported file does not exist"}"#;

    fn roots() -> Vec<String> {
        vec!["C:/work/repo".to_string()]
    }

    #[test]
    fn the_real_output_shape_gives_a_finding_named_below_the_project() {
        let report = parse_report(BREAK, &roots(), "svc/", &known(&["svc/a/s.proto"])).unwrap();
        assert_eq!(report.unread, None);
        assert_eq!(
            report.hits,
            [Hit {
                rule: "FIELD_SAME_TYPE".into(),
                file: "a/s.proto".into(),
                line: 7,
                message: "Field \"2\" with name \"age\" on message \"Foo\" changed type from \"int32\" to \"string\".".into(),
            }]
        );
    }

    #[test]
    fn a_compile_entry_is_not_a_finding_but_is_remembered() {
        let text = format!("{COMPILE}\n{BREAK}\n");
        let report = parse_report(&text, &roots(), "svc/", &known(&["svc/a/s.proto"])).unwrap();
        assert_eq!(report.hits.len(), 1);
        let unread = report.unread.unwrap();
        assert!(
            unread.contains("svc/a/imp.proto") && unread.contains("imported file does not exist"),
            "{unread}"
        );
        let none = parse_report(COMPILE, &roots(), "svc/", &known(&[])).unwrap();
        assert!(none.hits.is_empty() && none.unread.is_some());
    }

    #[test]
    fn a_file_that_neither_the_index_nor_the_baseline_has_is_not_reported() {
        let report = parse_report(BREAK, &roots(), "svc/", &known(&["svc/a/other.proto"])).unwrap();
        assert!(report.hits.is_empty());
    }

    #[test]
    fn output_that_is_not_buf_json_is_unreadable_not_clean() {
        for text in ["not json", "{\"path\":1}", "[]"] {
            let why = parse_report(text, &roots(), "", &known(&[])).unwrap_err();
            assert!(why.contains("not a buf violation"), "{why}");
        }
    }

    #[test]
    fn a_deleted_file_has_no_path_and_is_named_from_its_message() {
        let text = r#"{"start_line":1,"start_column":1,"end_line":1,"end_column":1,"type":"FILE_NO_DELETE","message":"Previously present file \"a/v1/gone.proto\" was deleted."}"#;
        let one = known(&["svc/proto/a/v1/gone.proto", "svc/proto/a/v1/other.proto"]);
        let report = parse_report(text, &[], "svc/", &one).unwrap();
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].file, "proto/a/v1/gone.proto");
        // named by two files: not guessed, the finding is kept without a place
        let two = known(&["x/a/v1/gone.proto", "y/a/v1/gone.proto"]);
        let report = parse_report(text, &[], "", &two).unwrap();
        assert_eq!((report.hits.len(), report.hits[0].file.as_str()), (1, ""));
    }

    #[test]
    fn a_line_zero_is_line_one() {
        let text = r#"{"path":"a.proto","start_line":0,"type":"FILE_NO_DELETE","message":"gone"}"#;
        let report = parse_report(text, &[], "", &known(&["a.proto"])).unwrap();
        assert_eq!(report.hits[0].line, 1);
    }

    #[test]
    fn the_comparison_names_the_common_git_directory_and_the_commit() {
        let plan = Plan {
            repo: Repo {
                top: PathBuf::from("/work/repo"),
                common: PathBuf::from("/work/repo/.git"),
                prefix: String::new(),
                head: "h".into(),
            },
            sha: "abc".into(),
            how: "x".into(),
            roots: vec![String::new()],
            removed: vec![],
            known: HashSet::new(),
        };
        let args: Vec<String> = check_args(&plan)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "breaking");
        assert_eq!(args[1], PathBuf::from("/work/repo").display().to_string());
        let against = args.iter().position(|a| a == "--against").unwrap();
        assert!(args[against + 1].ends_with(".git#ref=abc"), "{args:?}");
        for flag in ["--config", "--against-config"] {
            let at = args.iter().position(|a| a == flag).unwrap();
            assert!(args[at + 1].contains("\"FILE\""), "{flag}");
        }
        assert!(args.windows(2).any(|w| w == ["--error-format", "json"]));
    }

    #[test]
    fn the_gap_for_a_tree_with_nothing_to_compare_says_what_to_do() {
        let why = not_distinct("the merge-base of HEAD with origin/main (abc), which is HEAD");
        assert!(
            why.contains("no baseline distinct from the tree") && why.contains("[buf] baseline"),
            "{why}"
        );
    }
}
