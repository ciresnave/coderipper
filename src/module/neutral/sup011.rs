//! SUP-011: analysis tools skip the source the project merely vendors.
//!
//! **What this judges:** a tracked directory named `vendor`, `vendors`, `third_party`, `third-party`, `node_modules` or
//! `bower_components` (the shallowest such segment on a path) holds vendored code. For each analysis-tool configuration the
//! repository tracks at its root (`codecov.yml`, `.codecov.yml`, `codecov.yaml`, `.codacy.yml`, `.codacy.yaml`,
//! `sonar-project.properties`, `.eslintignore`, `.deepsource.toml`), the vendored directory's name must appear in it. One finding
//! per configuration and directory.
//!
//! **What it cannot see:** whether the name appears *as an exclusion* (the file is searched as text, so a mention in a comment
//! satisfies it); tools configured elsewhere (CI flags, a user-level file); directories vendored under another name. A repository
//! that vendors code but tracks no analysis configuration is `not applicable`: there is nothing to exclude it from.

use super::{file_name, RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = "SUP-011";

const VENDOR_SEGMENTS: &[&str] = &[
    "vendor",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bower_components",
];

const CONFIGS: &[&str] = &[
    "codecov.yml",
    ".codecov.yml",
    "codecov.yaml",
    ".codacy.yml",
    ".codacy.yaml",
    "sonar-project.properties",
    ".eslintignore",
    ".deepsource.toml",
];

/// The vendored directories among the tracked files, each with how many files it holds: the path up to and including the
/// shallowest directory segment that names vendored code.
fn vendored_directories(view: &RepoView) -> Vec<(String, usize)> {
    let mut found: Vec<(String, usize)> = Vec::new();
    for path in view.paths() {
        let segments: Vec<&str> = path.split('/').collect();
        // the last segment is the file's own name: a file called `vendor` is not a directory
        let Some(at) = segments[..segments.len() - 1]
            .iter()
            .position(|s| VENDOR_SEGMENTS.contains(s))
        else {
            continue;
        };
        let dir = segments[..=at].join("/");
        match found.iter_mut().find(|(d, _)| *d == dir) {
            Some(entry) => entry.1 += 1,
            None => found.push((dir, 1)),
        }
    }
    found
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let vendored = vendored_directories(view);
    if vendored.is_empty() {
        return Verdict::Findings(Vec::new());
    }
    let configs: Vec<&str> = CONFIGS.iter().copied().filter(|c| view.has(c)).collect();
    if configs.is_empty() {
        return Verdict::NotApplicable(
            "vendored code is tracked, but the repository tracks no analysis-tool configuration to exclude it from".into(),
        );
    }
    let mut findings = Vec::new();
    for config in configs {
        let Some(text) = view.read(config) else {
            continue;
        };
        for (dir, files) in &vendored {
            let name = file_name(dir);
            if text.contains(name) {
                continue;
            }
            findings.push(
                Finding::new(
                    ID,
                    Severity::Low,
                    Confidence::Medium,
                    view.name(),
                    format!("{config} does not exclude the vendored directory {dir}"),
                    format!(
                        "{dir} holds third-party code the team does not maintain, and judging it by the team's own standards buries real findings in noise. Add `{name}` to the exclusions in {config}."
                    ),
                )
                .location(Location::new(config, None))
                .subject(dir.as_str())
                .positive_control(format!(
                    "{dir} holds {files} tracked file(s) and {config} was read; the search was for the text `{name}`"
                )),
            );
        }
    }
    Verdict::Findings(findings)
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

    #[test]
    fn a_vendored_directory_the_config_does_not_mention_is_reported() {
        let found = run(&[
            ("codecov.yml", "coverage:\n  status:\n    project: off\n"),
            ("vendor/lib/a.c", "int a;"),
            ("src/main.c", "int main;"),
        ]);
        assert_eq!(found.len(), 1, "{found:?}");
        let f = &found[0];
        assert_eq!(f.check_id, "SUP-011");
        assert_eq!(f.location.as_ref().unwrap().file, "codecov.yml");
        assert_eq!(f.subject.as_deref(), Some("vendor"));
        assert!(f.clone().validate().is_ok());
    }

    #[test]
    fn a_config_that_names_the_directory_is_clean() {
        let found = run(&[
            ("codecov.yml", "ignore:\n  - \"vendor/**\"\n"),
            ("vendor/lib/a.c", "int a;"),
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn each_config_and_each_directory_is_judged_alone() {
        let found = run(&[
            ("codecov.yml", "ignore:\n  - vendor\n"),
            (".eslintignore", "dist\n"),
            ("vendor/a.js", ""),
            ("web/node_modules/b.js", ""),
            ("third_party/c.js", ""),
        ]);
        let mut got: Vec<(String, String)> = found
            .iter()
            .map(|f| {
                (
                    f.location.as_ref().unwrap().file.clone(),
                    f.subject.clone().unwrap(),
                )
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                (".eslintignore".into(), "third_party".into()),
                (".eslintignore".into(), "vendor".into()),
                (".eslintignore".into(), "web/node_modules".into()),
                ("codecov.yml".into(), "third_party".into()),
                ("codecov.yml".into(), "web/node_modules".into()),
            ]
        );
    }

    #[test]
    fn no_vendored_directory_is_clean() {
        let found = run(&[("codecov.yml", "x: 1\n"), ("src/main.c", "")]);
        assert!(found.is_empty());
    }

    #[test]
    fn vendored_code_with_no_analysis_config_is_not_applicable() {
        let view = RepoView::from_files(&[("vendor/a.c", ""), ("src/main.c", "")]);
        assert!(matches!(judge(&view), Verdict::NotApplicable(_)));
    }

    #[test]
    fn a_file_merely_named_vendor_is_not_a_directory() {
        let found = run(&[("codecov.yml", "x: 1\n"), ("docs/vendor", "notes")]);
        assert!(found.is_empty(), "{found:?}");
    }
}
