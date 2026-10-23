// SPDX-License-Identifier: MIT OR Apache-2.0
//! [`NeutralModule`]: the language-neutral rules, built in. They read the repository's tracked files (manifests, lockfiles, CI
//! workflows, documents), never a parser's output, so one implementation serves every language.
//!
//! The module answers the same protocol as [`super::RustModule`], in process. Every rule id it claims is a catalog id
//! (`SUP-001`), and each claim carries the conformance fixture that earns it (`conformance/<rule>/`, run by this repository's own
//! tests). What each rule judges is stated in its own file, with what it cannot see: these are narrow, deterministic readings
//! of broader rules, and the module says so rather than claiming the whole rule.
//!
//! Files are read from `HEAD` (`git ls-tree`, `git show`), as the Rust checks analyse `HEAD`, so a dirty working tree changes
//! nothing. A project that is not a git repository with a commit is an error for every rule, never a clean run.

mod ci_security;
mod conflict_marker;
mod doc005;
mod doc009;
mod licence;
mod lint_suppression;
mod patent_index;
mod sup001;
mod sup004;
mod sup011;
#[cfg(test)]
mod variants;
mod version_changelog;
mod wsp001;

use super::{
    Capabilities, ErrorKind, Event, Hello, Module, ModuleOutput, ModuleSummary, Request, RuleClaim,
    RuleRan, RuleResult,
};
use crate::finding::Finding;
#[cfg(test)]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What a rule concluded about the repository it was shown.
pub(crate) enum Verdict {
    /// It ran; these are its findings (none means the rule is clean).
    Findings(Vec<Finding>),
    /// As `Findings`, with notes the run prints beside them (they never change the verdict).
    Noted(Vec<Finding>, Vec<String>),
    /// The rule does not apply to this repository (the reason is shown). A coverage gap, never "clean".
    NotApplicable(String),
}

/// One rule: its catalog id and the function that judges a repository.
struct Rule {
    id: &'static str,
    judge: fn(&RepoView) -> Verdict,
}

const RULES: &[Rule] = &[
    Rule {
        id: "SUP-001",
        judge: sup001::judge,
    },
    Rule {
        id: "SUP-004",
        judge: sup004::judge,
    },
    Rule {
        id: "SUP-011",
        judge: sup011::judge,
    },
    Rule {
        id: "DOC-005",
        judge: doc005::judge,
    },
    Rule {
        id: "DOC-009",
        judge: doc009::judge,
    },
    Rule {
        id: "WSP-001",
        judge: wsp001::judge,
    },
    Rule {
        id: "lint-suppression",
        judge: lint_suppression::judge,
    },
    Rule {
        id: "conflict-marker",
        judge: conflict_marker::judge,
    },
    Rule {
        id: "patent-index-match",
        judge: patent_index::judge,
    },
    Rule {
        id: "licence",
        judge: licence::judge,
    },
    Rule {
        id: "ci-security",
        judge: ci_security::judge,
    },
    Rule {
        id: "version-and-changelog",
        judge: version_changelog::judge,
    },
];

/// The rules this module can check, in the order it reports them.
pub fn rule_ids() -> Vec<&'static str> {
    RULES.iter().map(|r| r.id).collect()
}

/// Where a [`RepoView`] gets its files.
enum Source {
    /// `HEAD` of the git repository at this directory.
    Git(PathBuf),
    /// Files held in memory (unit tests).
    #[cfg(test)]
    Memory(BTreeMap<String, String>),
}

/// The tracked files of a repository at `HEAD`: the list of paths, and their contents on demand.
pub(crate) struct RepoView {
    name: String,
    files: Vec<String>,
    source: Source,
    /// The tree a branch started from, for tests (`None`: no base can be found).
    #[cfg(test)]
    base: Option<BTreeMap<String, String>>,
}

/// The branches whose merge-base with `HEAD` is where a change started, in the order they are tried.
const DEFAULT_BRANCHES: &[&str] = &["origin/HEAD", "origin/main", "origin/master"];

/// Where [`RepoView::base`] got the tree a branch started from.
pub(crate) enum Base {
    /// The tree of the merge-base commit.
    Tree(BaseTree),
    /// `HEAD` is the base, so there is nothing to compare (a run on the default branch). The reason is shown.
    SameAsHead(String),
    /// No base could be found (no default branch, or a clone without the history). A coverage gap, never "clean".
    Gap(String),
}

/// The files of the commit a branch started from.
pub(crate) struct BaseTree {
    how: String,
    source: BaseSource,
}

enum BaseSource {
    Git {
        root: PathBuf,
        sha: String,
    },
    #[cfg(test)]
    Memory(BTreeMap<String, String>),
}

impl BaseTree {
    /// How the base was chosen, for a message: `the merge-base of HEAD with origin/main (abc1234)`.
    pub(crate) fn how(&self) -> &str {
        &self.how
    }

    /// Every path the base tracked, sorted, with forward slashes; empty when git cannot list the tree.
    pub(crate) fn paths(&self) -> Vec<String> {
        match &self.source {
            #[cfg(test)]
            BaseSource::Memory(map) => map.keys().cloned().collect(),
            BaseSource::Git { root, sha } => {
                let Ok(output) = crate::conformance::git_command(root)
                    .args(["ls-tree", "-r", "-z", "--name-only", sha])
                    .output()
                else {
                    return Vec::new();
                };
                if !output.status.success() {
                    return Vec::new();
                }
                let mut paths: Vec<String> = output
                    .stdout
                    .split(|&byte| byte == 0)
                    .filter(|path| !path.is_empty())
                    .map(|path| String::from_utf8_lossy(path).into_owned())
                    .collect();
                paths.sort();
                paths
            }
        }
    }

    /// The text of a file as the base had it; `None` when the base did not have it.
    pub(crate) fn read(&self, path: &str) -> Option<String> {
        match &self.source {
            #[cfg(test)]
            BaseSource::Memory(map) => map.get(path).cloned(),
            BaseSource::Git { root, sha } => {
                let output = crate::conformance::git_command(root)
                    .arg("show")
                    .arg(format!("{sha}:./{path}"))
                    .output()
                    .ok()?;
                output.status.success().then(|| {
                    let text = String::from_utf8_lossy(&output.stdout);
                    text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
                })
            }
        }
    }
}

/// Stdout of `git` in `root`, trimmed; the error is git's own message.
fn git_out(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = crate::conformance::git_command(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

impl RepoView {
    /// The repository at `root`, as committed at `HEAD`.
    pub(crate) fn from_git(root: &Path) -> anyhow::Result<Self> {
        let output = crate::conformance::git_command(root)
            .args(["ls-tree", "-r", "-z", "--name-only", "HEAD"])
            .output()
            .map_err(|e| anyhow::anyhow!("cannot run git: {e}"))?;
        if !output.status.success() {
            anyhow::bail!(
                "{} is not a git repository with a commit (the rules read HEAD): {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect();
        files.sort();
        Ok(Self {
            name: root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            files,
            source: Source::Git(root.to_path_buf()),
            #[cfg(test)]
            base: None,
        })
    }

    /// The commit this branch started from: the merge-base of `HEAD` with `origin/HEAD`, else `origin/main`, else `origin/master`.
    pub(crate) fn base(&self) -> Base {
        #[cfg(test)]
        let Source::Git(root) = &self.source
        else {
            return match &self.base {
                Some(map) => Base::Tree(BaseTree {
                    how: "the test base".into(),
                    source: BaseSource::Memory(map.clone()),
                }),
                None => Base::Gap("the test view has no base".into()),
            };
        };
        #[cfg(not(test))]
        let Source::Git(root) = &self.source;
        for candidate in DEFAULT_BRANCHES {
            let spec = format!("{candidate}^{{commit}}");
            if git_out(root, &["rev-parse", "--verify", "-q", &spec]).is_err() {
                continue;
            }
            let Ok(head) = git_out(root, &["rev-parse", "HEAD"]) else {
                break;
            };
            return match git_out(root, &["merge-base", "HEAD", candidate]) {
                Ok(sha) if sha == head => Base::SameAsHead(format!(
                    "HEAD is the merge-base of HEAD with {candidate}, so there is no branch to compare (a run on the default branch)"
                )),
                Ok(sha) => Base::Tree(BaseTree {
                    how: format!(
                        "the merge-base of HEAD with {candidate} ({})",
                        sha.chars().take(7).collect::<String>()
                    ),
                    source: BaseSource::Git {
                        root: root.clone(),
                        sha,
                    },
                }),
                Err(_) => Base::Gap(format!(
                    "HEAD and {candidate} share no commit here (a shallow clone does not have the history): fetch the full history (`fetch-depth: 0`)"
                )),
            };
        }
        Base::Gap(
            "the repository has no origin/HEAD, origin/main or origin/master to take a base from"
                .into(),
        )
    }

    /// A repository made of these `(path, contents)` pairs.
    #[cfg(test)]
    pub(crate) fn from_files(files: &[(&str, &str)]) -> Self {
        let map: BTreeMap<String, String> = files
            .iter()
            .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
            .collect();
        Self {
            name: "repo".into(),
            files: map.keys().cloned().collect(),
            source: Source::Memory(map),
            base: None,
        }
    }

    /// As [`RepoView::from_files`], with the files the branch started from (`None`: no base).
    #[cfg(test)]
    pub(crate) fn from_files_with_base(
        files: &[(&str, &str)],
        base: Option<&[(&str, &str)]>,
    ) -> Self {
        let mut view = Self::from_files(files);
        view.base = base.map(|b| {
            b.iter()
                .map(|(p, c)| ((*p).to_string(), (*c).to_string()))
                .collect()
        });
        view
    }

    /// The project's directory name (what a finding's `project` says).
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Every tracked path, sorted, with forward slashes.
    pub(crate) fn paths(&self) -> &[String] {
        &self.files
    }

    /// Whether `path` is tracked.
    pub(crate) fn has(&self, path: &str) -> bool {
        self.files
            .binary_search_by(|f| f.as_str().cmp(path))
            .is_ok()
    }

    /// The tracked paths with a line matching `pattern` (an extended regular expression), asked of git in one call so that a
    /// rule need not read every blob. `None` when git cannot say (or this view is not a git repository): the caller then reads
    /// every file. Every path git names must be one this view tracks, or the answer is not trusted.
    pub(crate) fn paths_matching(&self, pattern: &str) -> Option<Vec<String>> {
        // an in-memory view (tests only) has no git to ask
        #[cfg(test)]
        let Source::Git(root) = &self.source
        else {
            return None;
        };
        #[cfg(not(test))]
        let Source::Git(root) = &self.source;
        let output = crate::conformance::git_command(root)
            .args(["grep", "-l", "-z", "-I", "-E", pattern, "HEAD", "--", "."])
            .output()
            .ok()?;
        // exit 1 is "no match"; anything else that is not success is a failure
        if !output.status.success() && output.status.code() != Some(1) {
            return None;
        }
        let mut found = Vec::new();
        for entry in String::from_utf8_lossy(&output.stdout).split(' ') {
            if entry.is_empty() {
                continue;
            }
            let path = entry.strip_prefix("HEAD:")?;
            if !self.has(path) {
                return None;
            }
            found.push(path.to_string());
        }
        Some(found)
    }

    /// The text of a tracked file (invalid UTF-8 is replaced); `None` when it is not tracked or cannot be read.
    pub(crate) fn read(&self, path: &str) -> Option<String> {
        if !self.has(path) {
            return None;
        }
        match &self.source {
            #[cfg(test)]
            Source::Memory(map) => map.get(path).cloned(),
            Source::Git(root) => {
                let output = crate::conformance::git_command(root)
                    .arg("show")
                    // `./`: relative to the project directory, which may be a subdirectory of the repository
                    .arg(format!("HEAD:./{path}"))
                    .output()
                    .ok()?;
                output.status.success().then(|| {
                    let text = String::from_utf8_lossy(&output.stdout);
                    text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
                })
            }
        }
    }
}

/// The directory part of a tracked path (`""` for a file at the root).
pub(crate) fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file-name part of a path.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `dir` and each of its ancestors, nearest first, ending with the root (`""`).
pub(crate) fn dir_and_ancestors(dir: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut current = dir;
    loop {
        found.push(current.to_string());
        if current.is_empty() {
            return found;
        }
        current = dir_of(current);
    }
}

/// `dir/name`, or just `name` for the root.
pub(crate) fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The language-neutral rules, answering the module protocol in process.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeutralModule;

impl NeutralModule {
    /// The module.
    pub fn new() -> Self {
        Self
    }
}

/// The rules whose claim is earned: each has a seeded-defect fixture and a clean twin under `conformance/`
/// (`tests/neutral_module.rs` fails if this list and the fixtures disagree).
/// Rules whose check reads the files of some languages only (the coverage report shows the others as gaps). `SUP-001` reads
/// Cargo manifests and lockfiles (`rust`) and npm-family manifests and lockfiles (`typescript`, which stands for the
/// JavaScript family); Python, Go and the rest are not read.
const READS_ONLY: &[(&str, &[&str])] = &[
    ("SUP-001", &["rust", "typescript"]),
    ("patent-index-match", &["rust", "typescript", "python"]),
];

const PROVEN: &[&str] = &[
    "SUP-001",
    "SUP-004",
    "SUP-011",
    "DOC-005",
    "DOC-009",
    "WSP-001",
    "lint-suppression",
    "conflict-marker",
    "patent-index-match",
    "licence",
    "ci-security",
    "version-and-changelog",
];

impl Module for NeutralModule {
    fn describe(&self) -> anyhow::Result<Hello> {
        Ok(Hello::new(
            "neutral",
            env!("CARGO_PKG_VERSION"),
            vec!["any".into()],
            vec![],
            // reads files and runs nothing of the project's
            Capabilities::new(false, false, false, false),
            RULES
                .iter()
                .map(|r| {
                    let mut claim = RuleClaim::native(r.id);
                    if let Some((_, languages)) = READS_ONLY.iter().find(|(id, _)| *id == r.id) {
                        claim = claim.only_languages(languages);
                    }
                    if PROVEN.contains(&r.id) {
                        claim.proof = Some(format!("conformance/{}", r.id));
                    }
                    claim
                })
                .collect(),
        ))
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let view = match RepoView::from_git(&request.project_root) {
            Ok(view) => view,
            Err(e) => {
                // every requested rule gets the same error, so the host sees an error per rule and never a clean run
                let mut events: Vec<Event> = request
                    .rules
                    .iter()
                    .map(|rule| {
                        Event::RuleResult(RuleResult::error(
                            rule,
                            ErrorKind::Internal,
                            e.to_string(),
                        ))
                    })
                    .collect();
                events.push(Event::Summary(ModuleSummary {
                    rules_requested: request.rules.len(),
                    rules_errored: request.rules.len(),
                    ..ModuleSummary::default()
                }));
                return ModuleOutput {
                    in_process: true,
                    ..ModuleOutput::from_events(events)
                };
            }
        };
        let mut events = Vec::new();
        let mut summary = ModuleSummary {
            rules_requested: request.rules.len(),
            ..ModuleSummary::default()
        };
        for rule in &request.rules {
            let Some(known) = RULES.iter().find(|r| r.id == rule) else {
                events.push(Event::RuleResult(RuleResult::error(
                    rule,
                    ErrorKind::Internal,
                    "the neutral module has no such rule",
                )));
                summary.rules_errored += 1;
                continue;
            };
            match (known.judge)(&view) {
                Verdict::Findings(found) => {
                    summary.rules_ran += 1;
                    summary.findings += found.len();
                    let count = found.len();
                    events.extend(found.into_iter().map(|f| Event::Finding(Box::new(f))));
                    events.push(Event::RuleResult(RuleResult::ran(rule, count)));
                }
                Verdict::Noted(found, notes) => {
                    summary.rules_ran += 1;
                    summary.findings += found.len();
                    let count = found.len();
                    events.extend(found.into_iter().map(|f| Event::Finding(Box::new(f))));
                    events.push(Event::RuleResult(RuleResult::ran_noted(
                        rule, count, &notes,
                    )));
                }
                Verdict::NotApplicable(why) => {
                    summary.rules_skipped += 1;
                    events.push(Event::RuleResult(RuleResult {
                        status: RuleRan::Skipped,
                        reason_code: Some("not_applicable_here".into()),
                        detail: Some(why),
                        ..RuleResult::ran(rule, 0)
                    }));
                }
            }
        }
        events.push(Event::Summary(summary));
        ModuleOutput {
            in_process: true,
            ..ModuleOutput::from_events(events)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_split_into_directory_and_name() {
        assert_eq!(dir_of("a/b/c.md"), "a/b");
        assert_eq!(dir_of("c.md"), "");
        assert_eq!(file_name("a/b/c.md"), "c.md");
        assert_eq!(join("", "x"), "x");
        assert_eq!(join("a", "x"), "a/x");
        assert_eq!(dir_and_ancestors("a/b"), vec!["a/b", "a", ""]);
        assert_eq!(dir_and_ancestors(""), vec![""]);
    }

    #[test]
    fn the_repo_view_reads_only_tracked_files() {
        let view = RepoView::from_files(&[("a.txt", "A"), ("d/b.txt", "B")]);
        assert_eq!(view.read("a.txt").as_deref(), Some("A"));
        assert_eq!(view.read("d/b.txt").as_deref(), Some("B"));
        assert_eq!(view.read("nope.txt"), None);
        assert!(view.has("a.txt") && !view.has("nope.txt"));
    }

    #[test]
    fn the_module_claims_exactly_its_rules_all_proven() {
        let hello = NeutralModule::new().describe().unwrap();
        let claimed: Vec<&str> = hello.rules.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(claimed, rule_ids());
        assert_eq!(rule_ids().len(), 12);
        assert!(hello.rules.iter().all(|r| r.proof.is_some()));
        assert!(hello.problem().is_none());
    }
}
