//! SUP-001: a project that ships as an application commits its lockfile, and its CI installs strictly from it.
//!
//! **What this judges (two readings of the rule, both from tracked files):**
//!
//! 1. *The lockfile is committed.* An application is a Cargo package with `src/main.rs` or a `[[bin]]` target, or an npm package
//!    with `"private": true`. It needs `Cargo.lock` / one of npm's, yarn's, pnpm's or bun's lockfiles tracked in its directory or
//!    in an ancestor (a workspace keeps one lock at its root). A library, and an npm package that is not private (published as a
//!    library), are not judged.
//! 2. *CI installs strictly.* In `.github/workflows/*.yml|yaml`, a command line that runs `npm install` with no package named
//!    (use `npm ci`), or `yarn install` without `--frozen-lockfile` / `--immutable` (when a `yarn.lock` is tracked), or a `cargo`
//!    build, test, check, clippy, run or doc without `--locked` / `--frozen` (when an application with a tracked `Cargo.lock`
//!    exists), resolves afresh and so does not fail when lockfile and manifest disagree. One finding per workflow file and tool.
//!
//! (`cargo bench` is in that list too, and the `npm install` reading applies only when an npm lockfile is tracked.)
//!
//! **What it cannot see:** other ecosystems (Go's `go.sum`, Python's lockfiles, Gradle) are not judged; a Cargo.toml that does not
//! parse, or a package.json whose `private` is not the boolean `true`, is not judged; a CI system other than GitHub Actions is not
//! read; a command built by a script or a matrix is not seen. The workflow text is read line by line, not as YAML (a line ending in
//! a backslash is joined to the next), a command must begin a line, follow `run:` or `- `, or follow `&&`, `||`, `;` or `|`
//! (so `run: "cargo build"` in quotes, and a command behind `VAR="a b" cargo ...`, are missed), and a folded `>` string is read
//! as the lines it is written on.

use super::{dir_and_ancestors, dir_of, file_name, join, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = "SUP-001";

const NODE_LOCKS: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
];

pub(super) fn judge(view: &RepoView) -> Verdict {
    let manifests: Vec<&String> = view
        .paths()
        .iter()
        .filter(|p| matches!(file_name(p), "Cargo.toml" | "package.json"))
        .collect();
    if manifests.is_empty() {
        return Verdict::NotApplicable(
            "no Cargo.toml or package.json is tracked, so none of the ecosystems this rule reads is present"
                .into(),
        );
    }
    let mut findings = Vec::new();
    let mut locked_cargo_application = false;
    for path in manifests {
        let dir = dir_of(path);
        let Some(text) = view.read(path) else {
            continue;
        };
        let ancestors = dir_and_ancestors(dir);
        if file_name(path) == "Cargo.toml" {
            if !is_cargo_application(view, dir, &text) {
                continue;
            }
            if ancestors.iter().any(|a| view.has(&join(a, "Cargo.lock"))) {
                locked_cargo_application = true;
            } else {
                findings.push(missing_lock(view, path, "Cargo.lock", "Cargo.lock"));
            }
        } else if is_private_npm_package(&text)
            && !ancestors
                .iter()
                .any(|a| NODE_LOCKS.iter().any(|lock| view.has(&join(a, lock))))
        {
            findings.push(missing_lock(
                view,
                path,
                "lockfile",
                "an npm, yarn, pnpm or bun lockfile",
            ));
        }
    }
    findings.extend(strict_install_findings(view, locked_cargo_application));
    Verdict::Findings(findings)
}

fn is_cargo_application(view: &RepoView, dir: &str, manifest: &str) -> bool {
    let Ok(table) = manifest.parse::<toml::Table>() else {
        return false;
    };
    table.contains_key("package")
        && (view.has(&join(dir, "src/main.rs"))
            || table
                .get("bin")
                .and_then(toml::Value::as_array)
                .is_some_and(|bins| !bins.is_empty()))
}

fn is_private_npm_package(manifest: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(manifest)
        .is_ok_and(|v| v.get("private") == Some(&serde_json::Value::Bool(true)))
}

fn missing_lock(view: &RepoView, manifest: &str, subject: &str, what: &str) -> Finding {
    Finding::new(
        ID,
        Severity::Medium,
        Confidence::Medium,
        view.name(),
        format!("{manifest} declares an application but {what} is not committed"),
        format!(
            "Without a committed lockfile every build may resolve different transitive code, so a build cannot be reproduced. Commit {what} next to the manifest or at the workspace root (and take it out of .gitignore)."
        ),
    )
    .location(Location::new(manifest, None))
    .subject(subject)
    .positive_control(format!(
        "{manifest} is tracked and was read: it declares an application, and the search for a lockfile covered its directory and every directory above it"
    ))
}

/// One finding per workflow file and tool that resolves dependencies afresh.
fn strict_install_findings(view: &RepoView, locked_cargo_application: bool) -> Vec<Finding> {
    let tracked = |name: &str| view.paths().iter().any(|p| file_name(p) == name);
    let npm_lock = tracked("package-lock.json") || tracked("npm-shrinkwrap.json");
    let yarn_lock = tracked("yarn.lock");
    let mut findings = Vec::new();
    for path in view.paths().iter().filter(|p| is_workflow(p)) {
        let Some(text) = view.read(path) else {
            continue;
        };
        // (tool, first line, count)
        let mut seen: Vec<(&'static str, usize, usize)> = Vec::new();
        for (line_number, raw) in logical_lines(&text) {
            for tool in loose_commands(&raw, npm_lock, yarn_lock, locked_cargo_application) {
                match seen.iter_mut().find(|s| s.0 == tool) {
                    Some(entry) => entry.2 += 1,
                    None => seen.push((tool, line_number, 1)),
                }
            }
        }
        for (tool, line, count) in seen {
            let (what, fix) = match tool {
                "npm" => (
                    "npm install",
                    "use `npm ci`, which fails when package-lock.json and package.json disagree",
                ),
                "yarn" => (
                    "yarn install",
                    "add `--frozen-lockfile` (yarn 1) or `--immutable` (yarn 2+)",
                ),
                _ => (
                    "cargo",
                    "add `--locked` (or `--frozen`) so the build fails when Cargo.lock needs changes",
                ),
            };
            let times = if count == 1 { "time" } else { "times" };
            findings.push(
                Finding::new(
                    ID,
                    Severity::Medium,
                    Confidence::Medium,
                    view.name(),
                    format!("{path} runs {what} without the strict mode ({count} {times})"),
                    format!(
                        "CI that resolves afresh does not fail when the lockfile and the manifest disagree, so the build is not the one the lockfile describes: {fix}. First at line {line}."
                    ),
                )
                .location(Location::new(path.as_str(), u32::try_from(line).ok()))
                .subject(tool),
            );
        }
    }
    findings
}

/// The text's lines as `(first line number, text)`, with a line that ends in a backslash joined to the next one, so a flag on a
/// continuation line belongs to its command.
fn logical_lines(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut joining = false;
    for (index, line) in text.lines().enumerate() {
        let continued = line.trim_end().ends_with('\\');
        let piece = line.trim_end().trim_end_matches('\\');
        match out.last_mut() {
            Some(last) if joining => {
                last.1.push(' ');
                last.1.push_str(piece.trim_start());
            }
            _ => out.push((index + 1, piece.to_string())),
        }
        joining = continued;
    }
    out
}

fn is_workflow(path: &str) -> bool {
    path.strip_prefix(".github/workflows/").is_some_and(|rest| {
        !rest.contains('/') && (rest.ends_with(".yml") || rest.ends_with(".yaml"))
    })
}

/// The tools a workflow line runs without their strict mode. A command must begin a line, or follow `run:`, `- `, `&&`,
/// `||`, `;` or `|`, so prose (`name: npm install the things`) and comments are not commands.
fn loose_commands(
    raw: &str,
    npm_lock: bool,
    yarn_lock: bool,
    locked_cargo_application: bool,
) -> Vec<&'static str> {
    let line = match raw.find('#') {
        Some(0) => "",
        Some(i) if raw[..i].ends_with(char::is_whitespace) => &raw[..i],
        _ => raw,
    };
    let mut line = line.trim();
    while let Some(rest) = line.strip_prefix("- ") {
        line = rest.trim_start();
    }
    let line = line.strip_prefix("run:").map_or(line, str::trim_start);
    let mut found = Vec::new();
    for segment in line
        .split("&&")
        .flat_map(|s| s.split("||"))
        .flat_map(|s| s.split([';', '|']))
    {
        let tokens: Vec<&str> = segment
            .split_whitespace()
            .skip_while(|t| t.contains('=') || *t == "sudo")
            .collect();
        let args = |from: usize| tokens.get(from..).unwrap_or(&[]);
        match tokens.first().copied() {
            Some("npm")
                if npm_lock
                    && matches!(tokens.get(1), Some(&"install" | &"i"))
                    && !args(2)
                        .iter()
                        .any(|a| !a.starts_with('-') || matches!(*a, "-g" | "--global")) =>
            {
                found.push("npm");
            }
            Some("yarn")
                if yarn_lock
                    && (tokens.len() == 1 || tokens.get(1) == Some(&"install"))
                    && !args(1)
                        .iter()
                        .any(|a| matches!(*a, "--frozen-lockfile" | "--immutable")) =>
            {
                found.push("yarn");
            }
            Some("cargo") if locked_cargo_application => {
                let from = if tokens.get(1).is_some_and(|t| t.starts_with('+')) {
                    2
                } else {
                    1
                };
                if matches!(
                    tokens.get(from),
                    Some(&"build" | &"test" | &"check" | &"clippy" | &"run" | &"doc" | &"bench")
                ) && !args(from + 1)
                    .iter()
                    .any(|a| matches!(*a, "--locked" | "--frozen"))
                {
                    found.push("cargo");
                }
            }
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) => f,
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    const APP: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";
    const LIB: &str = "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n";

    fn at(found: &[Finding]) -> Vec<(String, Option<String>)> {
        found
            .iter()
            .map(|f| (f.location.as_ref().unwrap().file.clone(), f.subject.clone()))
            .collect()
    }

    #[test]
    fn a_cargo_application_without_a_committed_lockfile_is_reported() {
        let found = run(&[("Cargo.toml", APP), ("src/main.rs", "fn main() {}")]);
        assert_eq!(
            at(&found),
            vec![("Cargo.toml".into(), Some("Cargo.lock".into()))]
        );
        assert_eq!(found[0].check_id, "SUP-001");
        assert!(
            found[0].clone().validate().is_ok(),
            "an absence claim carries its control"
        );
    }

    #[test]
    fn a_cargo_application_with_its_lockfile_is_clean() {
        let found = run(&[
            ("Cargo.toml", APP),
            ("Cargo.lock", "# lock"),
            ("src/main.rs", "fn main() {}"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_bin_target_makes_an_application_without_main_rs() {
        let manifest = format!("{LIB}[[bin]]\nname = \"tool\"\npath = \"tool.rs\"\n");
        let found = run(&[("Cargo.toml", &manifest), ("tool.rs", "fn main() {}")]);
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_cargo_library_needs_no_lockfile() {
        let found = run(&[("Cargo.toml", LIB), ("src/lib.rs", "")]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_workspace_member_application_is_satisfied_by_the_root_lockfile() {
        let root = "[workspace]\nmembers = [\"crates/app\"]\n";
        let files = [
            ("Cargo.toml", root),
            ("crates/app/Cargo.toml", APP),
            ("crates/app/src/main.rs", "fn main() {}"),
        ];
        let found = run(&files);
        assert_eq!(
            at(&found),
            vec![("crates/app/Cargo.toml".into(), Some("Cargo.lock".into()))]
        );
        let mut with_lock = files.to_vec();
        with_lock.push(("Cargo.lock", "# lock"));
        assert!(run(&with_lock).is_empty());
    }

    #[test]
    fn a_private_npm_package_needs_a_lockfile_and_a_public_one_does_not() {
        let private = "{ \"name\": \"site\", \"private\": true }";
        let public = "{ \"name\": \"lib\", \"version\": \"1.0.0\" }";
        let found = run(&[("package.json", private)]);
        assert_eq!(
            at(&found),
            vec![("package.json".into(), Some("lockfile".into()))]
        );
        for lock in NODE_LOCKS {
            assert!(
                run(&[("package.json", private), (lock, "x")]).is_empty(),
                "{lock}"
            );
        }
        assert!(run(&[("package.json", public)]).is_empty());
    }

    fn workflow(body: &str) -> String {
        format!("name: ci\non: push\njobs:\n  build:\n    steps:\n{body}")
    }

    #[test]
    fn npm_install_in_ci_is_reported_and_npm_ci_is_not() {
        let manifest = "{ \"private\": true }";
        let base = |cmd: &str| {
            let wf = workflow(&format!("      - run: {cmd}\n"));
            run(&[
                ("package.json", manifest),
                ("package-lock.json", "{}"),
                (".github/workflows/ci.yml", &wf),
            ])
        };
        let found = base("npm install");
        assert_eq!(
            at(&found),
            vec![(".github/workflows/ci.yml".into(), Some("npm".into()))]
        );
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(6));
        assert!(base("npm ci").is_empty());
        assert!(
            base("npm install -g typescript").is_empty(),
            "a named package is a tool install"
        );
        assert!(base("npm install --global typescript").is_empty());
        assert_eq!(base("npm i").len(), 1);
    }

    #[test]
    fn a_command_inside_a_block_or_after_and_and_is_found_but_prose_and_comments_are_not() {
        let wf = workflow(
            "      - name: npm install the things\n        run: |\n          echo hi\n          cd web && npm install\n      # npm install (commented out)\n",
        );
        let found = run(&[
            ("package.json", "{ \"private\": true }"),
            ("package-lock.json", "{}"),
            (".github/workflows/ci.yaml", &wf),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        // the header is five lines, so the `cd web && npm install` line is the ninth
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(9));
    }

    #[test]
    fn a_flag_on_a_continuation_line_belongs_to_its_command() {
        let files = |cmd: &str| {
            let wf = workflow(&format!("      - run: |\n          {cmd}\n"));
            run(&[
                ("Cargo.toml", APP),
                ("Cargo.lock", "# lock"),
                ("src/main.rs", "fn main() {}"),
                (".github/workflows/ci.yml", &wf),
            ])
        };
        assert!(files("cargo build --release \\\n            --locked").is_empty());
        let found = files("cargo build --release \\\n            --all-features");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(7));
    }

    #[test]
    fn yarn_install_needs_a_frozen_lockfile_flag() {
        let files = |cmd: &str| {
            let wf = workflow(&format!("      - run: {cmd}\n"));
            run(&[
                ("package.json", "{ \"private\": true }"),
                ("yarn.lock", "x"),
                (".github/workflows/ci.yml", &wf),
            ])
        };
        assert_eq!(files("yarn install").len(), 1);
        assert!(files("yarn install --frozen-lockfile").is_empty());
        assert!(files("yarn install --immutable").is_empty());
    }

    #[test]
    fn cargo_in_ci_needs_locked_for_an_application_but_not_for_a_library() {
        let wf = |cmd: &str| workflow(&format!("      - run: {cmd}\n"));
        let app = |cmd: &str| {
            run(&[
                ("Cargo.toml", APP),
                ("Cargo.lock", "# lock"),
                ("src/main.rs", "fn main() {}"),
                (".github/workflows/ci.yml", &wf(cmd)),
            ])
        };
        let found = app("cargo test --all-features");
        assert_eq!(
            at(&found),
            vec![(".github/workflows/ci.yml".into(), Some("cargo".into()))]
        );
        assert!(app("cargo test --locked").is_empty());
        assert!(app("cargo build --release --frozen").is_empty());
        assert!(
            app("cargo fmt --check").is_empty(),
            "fmt does not resolve dependencies"
        );
        assert!(
            app("cargo install cargo-audit").is_empty(),
            "installing a tool is not a build of this project"
        );
        let lib = run(&[
            ("Cargo.toml", LIB),
            ("src/lib.rs", ""),
            (".github/workflows/ci.yml", &wf("cargo test")),
        ]);
        assert!(lib.is_empty(), "{lib:?}");
    }

    #[test]
    fn a_repository_with_no_manifest_the_rule_knows_is_not_applicable() {
        let view = RepoView::from_files(&[("README.md", "hi")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
    }
}
