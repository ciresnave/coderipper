// SPDX-License-Identifier: MIT OR Apache-2.0
//! `ci-security`: what a workflow gives a pull request from outside, on a repository anyone can fork.
//!
//! **What this judges (CireSnave, 2026-10-22: "Don't give pull-request-triggered jobs on a public repo any repository secrets. Prefer
//! hosted runners for public repos wherever a job doesn't need our GPU."), from the workflow files alone:**
//! - **High:** a job in a workflow triggered by `pull_request` or `pull_request_target` that reads a secret other than `GITHUB_TOKEN`
//!   (`secrets.NAME`, or `secrets: inherit` on a reusable-workflow call), through the job itself or the workflow's top-level `env`.
//! - **High:** a `pull_request_target` job whose `actions/checkout` step takes the pull request's head (`github.event.pull_request.head`,
//!   `github.head_ref`): code from outside runs with the base repository's token and secrets.
//! - **Medium:** a job that runs on a self-hosted or custom-labelled runner and mentions no GPU, CUDA, Vulkan, ROCm or OpenCL anywhere
//!   (its name, steps, labels, or the workflow's file name): a hosted runner would do. A runner chosen by an expression
//!   (`${{ matrix.os }}`) is not judged.
//! - **High:** a job on a self-hosted or custom-labelled runner under `pull_request` or `pull_request_target` whose `if:` lacks the
//!   same-repo condition (`github.event.pull_request.head.repo.full_name == github.repository`): a fork's pull request would run on
//!   the machine (CireSnave, 2026-10-16, the self-hosted policy 5.1h).
//! - **Medium:** the same job with the condition but no `environment:` (the required-reviewer gate; a private repository on the Free
//!   plan cannot have one, so its same-repo condition is the whole gate and this finding is waived there).
//! - **Medium:** a `pull_request` trigger whose `branches:` filter names neither `main` nor `master` (nor a catch-all pattern), or whose
//!   `branches-ignore:` names one of them: pull requests to the default branch get no checks (CireSnave, 2026-09-17, checks everywhere).
//! - **Low:** a workflow with no top-level `permissions:` block, so its token takes the repository default.
//!
//! Whether the repository is public is not known from its files, so every finding applies when it is; run the rule over the public
//! repositories. The repository setting "require approval for outside collaborators" is a forge setting, not a file, and is checked by
//! the settings audit, not here.
//!
//! **What it cannot see:** a secret reached through a composite action or a called workflow's own file; a runner label that resolves
//! through a matrix; the repository's visibility and settings; reusable workflows in other repositories.

use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use yaml_serde::Value;

const ID: &str = crate::rule_id::CI_SECURITY;

const GPU_WORDS: &[&str] = &["gpu", "cuda", "vulkan", "nvidia", "rocm", "opencl"];

fn key(map: &yaml_serde::Mapping, name: &str) -> Option<Value> {
    map.get(Value::String(name.to_string())).cloned()
}

/// The workflow files: `.github/workflows/*.yml` and `*.yaml` at the root of the repository.
fn workflow_files(view: &RepoView) -> Vec<String> {
    view.paths()
        .iter()
        .filter(|p| {
            p.strip_prefix(".github/workflows/").is_some_and(|rest| {
                !rest.contains('/') && (rest.ends_with(".yml") || rest.ends_with(".yaml"))
            })
        })
        .cloned()
        .collect()
}

/// The event names a workflow's `on:` lists (YAML 1.1 reads a bare `on` key as the boolean `true`).
fn events(doc: &yaml_serde::Mapping) -> Vec<String> {
    let on = key(doc, "on").or_else(|| doc.get(Value::Bool(true)).cloned());
    match on {
        Some(Value::String(s)) => vec![s],
        Some(Value::Sequence(seq)) => seq
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::Mapping(m)) => m
            .keys()
            .filter_map(|k| k.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// The `branches` / `branches-ignore` filters of the workflow's `pull_request` trigger, as `(branches, ignored)`; `None` when the
/// trigger is absent or carries no filter.
fn pull_request_filters(doc: &yaml_serde::Mapping) -> Option<(Vec<String>, Vec<String>)> {
    let on = key(doc, "on").or_else(|| doc.get(Value::Bool(true)).cloned())?;
    let Value::Mapping(on) = on else {
        return None;
    };
    let Some(Value::Mapping(pr)) = key(&on, "pull_request") else {
        return None;
    };
    let list = |name: &str| -> Vec<String> {
        match key(&pr, name) {
            Some(Value::Sequence(seq)) => seq
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(Value::String(s)) => vec![s],
            _ => Vec::new(),
        }
    };
    let (branches, ignored) = (list("branches"), list("branches-ignore"));
    (!branches.is_empty() || !ignored.is_empty()).then_some((branches, ignored))
}

/// Whether the job's `if:` carries the same-repo condition, with either operand order.
fn same_repo_condition(job: &yaml_serde::Mapping) -> bool {
    let Some(Value::String(cond)) = key(job, "if") else {
        return false;
    };
    let cond: String = cond.chars().filter(|c| !c.is_whitespace()).collect();
    cond.contains("github.event.pull_request.head.repo.full_name==github.repository")
        || cond.contains("github.repository==github.event.pull_request.head.repo.full_name")
}

fn dump(value: &Value) -> String {
    yaml_serde::to_string(value).unwrap_or_default()
}

/// The secrets (other than the workflow token) a piece of workflow text names.
fn secrets_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("secrets.") {
        // a longer identifier that merely ends in `secrets` (`mysecrets.x`) is not the context
        let before_ok = rest[..at]
            .chars()
            .last()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
        rest = &rest[at + "secrets.".len()..];
        if !before_ok {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !name.is_empty() && !name.eq_ignore_ascii_case("GITHUB_TOKEN") && !found.contains(&name)
        {
            found.push(name);
        }
    }
    found
}

/// Whether the job passes every secret on to a called workflow (`secrets: inherit`).
fn inherits(job: &yaml_serde::Mapping) -> bool {
    matches!(key(job, "secrets"), Some(Value::String(s)) if s == "inherit")
}

/// The labels a job's `runs-on` names; `None` when an expression chooses them.
fn labels(job: &yaml_serde::Mapping) -> Option<Vec<String>> {
    let labels: Vec<String> = match key(job, "runs-on")? {
        Value::String(s) => vec![s],
        Value::Sequence(seq) => seq
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Value::Mapping(m) => match key(&m, "labels") {
            Some(Value::String(s)) => vec![s],
            Some(Value::Sequence(seq)) => seq
                .into_iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        },
        _ => return None,
    };
    if labels.iter().any(|l| l.contains("${{")) {
        return None;
    }
    Some(labels)
}

fn hosted(label: &str) -> bool {
    ["ubuntu-", "windows-", "macos-"]
        .iter()
        .any(|p| label.to_ascii_lowercase().starts_with(p))
}

/// Whether a checkout step of the job takes the pull request's head.
fn checks_out_head(job: &yaml_serde::Mapping) -> bool {
    let Some(Value::Sequence(steps)) = key(job, "steps") else {
        return false;
    };
    steps.iter().any(|step| {
        let Value::Mapping(step) = step else {
            return false;
        };
        let uses = key(step, "uses")
            .and_then(|u| u.as_str().map(str::to_string))
            .unwrap_or_default();
        if !uses.starts_with("actions/checkout") {
            return false;
        }
        let Some(Value::Mapping(with)) = key(step, "with") else {
            return false;
        };
        ["ref", "repository"].iter().any(|k| {
            key(&with, k)
                .and_then(|v| v.as_str().map(str::to_string))
                .is_some_and(|v| v.contains("pull_request.head") || v.contains("github.head_ref"))
        })
    })
}

/// Whether the job's own `if:` limits it to named events and none of them is a pull request (`github.event_name == 'release'`): the
/// job is in a pull-request workflow but does not run for one. Anything more elaborate than a plain equality is not trusted.
fn never_on_pull_request(job: &yaml_serde::Mapping) -> bool {
    let Some(cond) = key(job, "if").and_then(|c| match c {
        Value::String(s) => Some(s),
        Value::Bool(_) => None,
        _ => None,
    }) else {
        return false;
    };
    // on a pull request `github.ref` is `refs/pull/N/merge`, so equality with a branch or tag ref is never true there
    let named_event = cond.contains("github.event_name ==");
    let named_ref = ["heads", "tags"].iter().any(|kind| {
        cond.contains(&format!("github.ref == 'refs/{kind}/"))
            || cond.contains(&format!("startsWith(github.ref, 'refs/{kind}/"))
    });
    (named_event || named_ref)
        && !cond.contains("pull_request")
        && !cond.contains("||")
        && !cond.contains("!=")
}

/// The 1-based line of a job's key in the workflow text (the first line that starts the name followed by a colon), or 1.
fn line_of(text: &str, needle: &str) -> u32 {
    let wanted = format!("{needle}:");
    text.lines()
        .position(|l| l.trim_start().trim_start_matches('"').starts_with(&wanted))
        .map_or(1, |i| i as u32 + 1)
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let files = workflow_files(view);
    if files.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no workflow under .github/workflows".into(),
        );
    }
    let project = view.name();
    let mut findings = Vec::new();
    let mut notes = Vec::new();
    let mut read = 0usize;
    for file in &files {
        let Some(text) = view.read(file) else {
            continue;
        };
        let doc = match yaml_serde::from_str::<Value>(&text) {
            Ok(Value::Mapping(m)) => m,
            _ => {
                notes.push(format!(
                    "{file} does not parse as a workflow and was not judged"
                ));
                continue;
            }
        };
        read += 1;
        let events = events(&doc);
        let from_outside = events
            .iter()
            .any(|e| e == "pull_request" || e == "pull_request_target");
        let target = events.iter().any(|e| e == "pull_request_target");
        let top_env_secrets = key(&doc, "env")
            .map(|e| secrets_in(&dump(&e)))
            .unwrap_or_default();
        let file_gpu = GPU_WORDS
            .iter()
            .any(|w| file.to_ascii_lowercase().contains(w))
            || key(&doc, "name")
                .and_then(|n| n.as_str().map(str::to_ascii_lowercase))
                .is_some_and(|n| GPU_WORDS.iter().any(|w| n.contains(w)));

        if key(&doc, "permissions").is_none() {
            findings.push(
                Finding::absence(
                    ID,
                    Severity::Low,
                    Confidence::High,
                    project,
                    format!("{file} has no top-level permissions block"),
                    "Without `permissions:` the workflow token takes the repository default, which may be read-write. Add `permissions: contents: read` at the top and widen it per job.",
                    format!("{file} was parsed and its top-level keys read; a `permissions` key is found where a workflow has one"),
                )
                .location(Location::new(file.clone(), Some(1)))
                .subject("permissions"),
            );
        }

        if let Some((branches, ignored)) = pull_request_filters(&doc) {
            let default = |b: &String| matches!(b.as_str(), "main" | "master" | "**" | "*");
            let leaves_out = if branches.is_empty() {
                ignored.iter().any(default)
            } else {
                !branches.iter().any(default)
            };
            if leaves_out {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Medium,
                        project,
                        format!("{file}: the pull_request trigger's branch filter leaves out main and master"),
                        "Every branch gets checks on pull requests, the default branch first. Drop the `branches:` filter, or list the default branch in it.",
                    )
                    .location(Location::new(file.clone(), Some(line_of(&text, "pull_request"))))
                    .subject("on:branches"),
                );
            }
        }

        let Some(Value::Mapping(jobs)) = key(&doc, "jobs") else {
            continue;
        };
        for (name, job) in &jobs {
            let (Some(name), Value::Mapping(job)) = (name.as_str(), job) else {
                continue;
            };
            let line = line_of(&text, name);
            if from_outside && !never_on_pull_request(job) {
                let mut secrets = secrets_in(&dump(&Value::Mapping(job.clone())));
                for s in &top_env_secrets {
                    if !secrets.contains(s) {
                        secrets.push(s.clone());
                    }
                }
                let inherit = inherits(job);
                if !secrets.is_empty() || inherit {
                    let what = if inherit {
                        "all repository secrets (`secrets: inherit`)".to_string()
                    } else {
                        format!("the secret(s) {}", secrets.join(", "))
                    };
                    findings.push(
                        Finding::new(
                            ID,
                            Severity::High,
                            Confidence::High,
                            project,
                            format!("{file}: job `{name}` runs on {} and reads {what}", events.join("/")),
                            "A pull request from outside the repository must not run with repository secrets. Move the step that needs the secret to a push or workflow_run job that runs after review, or drop the secret.",
                        )
                        .location(Location::new(file.clone(), Some(line)))
                        .subject(format!("{name}:secrets")),
                    );
                }
            }
            if target && checks_out_head(job) {
                findings.push(
                    Finding::new(
                        ID,
                        Severity::High,
                        Confidence::High,
                        project,
                        format!("{file}: pull_request_target job `{name}` checks out the pull request's head"),
                        "pull_request_target runs with the base repository's token and secrets; checking out the head runs the contributor's code there. Use pull_request, or check out the base and treat the head as data.",
                    )
                    .location(Location::new(file.clone(), Some(line)))
                    .subject(format!("{name}:checkout")),
                );
            }
            if let Some(labels) = labels(job) {
                let custom: Vec<&String> = labels.iter().filter(|l| !hosted(l)).collect();
                if !custom.is_empty() && from_outside && !never_on_pull_request(job) {
                    let on = custom
                        .iter()
                        .map(|l| format!("`{l}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let gated = same_repo_condition(job);
                    if !gated {
                        findings.push(
                            Finding::absence(
                                ID,
                                Severity::High,
                                Confidence::High,
                                project,
                                format!("{file}: job `{name}` runs on {on} under {} without the same-repo condition", events.join("/")),
                                "A pull request from a fork would run its workflow text on this machine. Add `if: github.event.pull_request.head.repo.full_name == github.repository` to the job (fork pull requests then get hosted-runner tests only), or move the job to push, schedule or workflow_dispatch.",
                                format!("{file} job `{name}` was read whole; its `if:` is searched for `github.event.pull_request.head.repo.full_name == github.repository` in either operand order, which the policy's own example carries"),
                            )
                            .location(Location::new(file.clone(), Some(line)))
                            .subject(format!("{name}:same-repo")),
                        );
                    } else if key(job, "environment").is_none() {
                        findings.push(
                            Finding::absence(
                                ID,
                                Severity::Medium,
                                Confidence::Medium,
                                project,
                                format!("{file}: job `{name}` runs on {on} under {} with no environment gate", events.join("/")),
                                "Add `environment:` naming an environment with a required reviewer, so a same-repo pull request waits for approval before it runs on the machine. A private repository on the Free plan cannot have required reviewers: waive this finding there, its same-repo condition is the gate.",
                                format!("{file} job `{name}` was read whole; its keys are searched for `environment`, which a gated job carries"),
                            )
                            .location(Location::new(file.clone(), Some(line)))
                            .subject(format!("{name}:environment")),
                        );
                    }
                }
                if !custom.is_empty() {
                    let haystack = format!("{}{}", dump(&Value::Mapping(job.clone())), "")
                        .to_ascii_lowercase();
                    let needs_gpu = file_gpu || GPU_WORDS.iter().any(|w| haystack.contains(w));
                    if !needs_gpu {
                        findings.push(
                            Finding::absence(
                                ID,
                                Severity::Medium,
                                Confidence::Medium,
                                project,
                                format!(
                                    "{file}: job `{name}` runs on {} and mentions no GPU",
                                    custom.iter().map(|l| format!("`{l}`")).collect::<Vec<_>>().join(", ")
                                ),
                                "On a public repository prefer a hosted runner (ubuntu-latest, windows-latest, macos-latest) for a job that needs no GPU: a self-hosted runner executes whatever a pull request's workflow says. Keep a self-hosted label only for a job that needs the GPU.",
                                format!("{file} job `{name}` was read whole; the search covers its name, steps, labels and the workflow's file name, and finds GPU, CUDA, Vulkan, ROCm and OpenCL where a job names them"),
                            )
                            .location(Location::new(file.clone(), Some(line)))
                            .subject(format!("{name}:runner")),
                        );
                    }
                }
            }
        }
    }
    notes.push(format!("{read} of {} workflow file(s) read", files.len()));
    Verdict::Noted(findings, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        match judge(&RepoView::from_files(files)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f
                .into_iter()
                .map(|f| f.validate().unwrap_or_else(|e| panic!("{e}")))
                .collect(),
            Verdict::NotApplicable(why) => panic!("not applicable: {why}"),
        }
    }

    fn subjects(found: &[Finding]) -> Vec<&str> {
        found
            .iter()
            .map(|f| f.subject.as_deref().unwrap_or(""))
            .collect()
    }

    const W: &str = ".github/workflows/ci.yml";

    const CLEAN: &str = "name: ci\non: [push, pull_request]\npermissions:\n  contents: read\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: echo ${{ secrets.GITHUB_TOKEN }}\n";

    #[test]
    fn a_clean_workflow_is_clean() {
        assert!(run(&[(W, CLEAN)]).is_empty());
    }

    #[test]
    fn no_workflow_is_not_applicable() {
        assert!(matches!(
            judge(&RepoView::from_files(&[("README.md", "x")])),
            Verdict::NotApplicable(_)
        ));
    }

    #[test]
    fn a_secret_in_a_pull_request_job_is_high() {
        let text = "on: pull_request\npermissions: {}\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: deploy\n        env:\n          TOKEN: ${{ secrets.DEPLOY_TOKEN }}\n";
        let found = run(&[(W, text)]);
        assert_eq!(subjects(&found), vec!["test:secrets"]);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(4));
        // positive control: the same job on push is not a pull-request job
        let on_push = text.replace("on: pull_request", "on: push");
        assert!(run(&[(W, &on_push)]).is_empty());
    }

    #[test]
    fn the_github_token_and_inherit_and_top_level_env() {
        let token = CLEAN.replace("on: [push, pull_request]", "on: pull_request");
        assert!(run(&[(W, &token)]).is_empty());
        let inherit = "on: pull_request\npermissions: {}\njobs:\n  call:\n    uses: ./.github/workflows/x.yml\n    secrets: inherit\n";
        assert_eq!(subjects(&run(&[(W, inherit)])), vec!["call:secrets"]);
        let env = "on:\n  pull_request:\n    branches: [main]\npermissions: {}\nenv:\n  K: ${{ secrets.K }}\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: x\n";
        assert_eq!(subjects(&run(&[(W, env)])), vec!["a:secrets"]);
    }

    #[test]
    fn pull_request_target_checking_out_the_head_is_high() {
        let text = "on: pull_request_target\npermissions: {}\njobs:\n  t:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          ref: ${{ github.event.pull_request.head.sha }}\n";
        let found = run(&[(W, text)]);
        assert_eq!(subjects(&found), vec!["t:checkout"]);
        // positive control: checking out the base is fine
        let base = text.replace("${{ github.event.pull_request.head.sha }}", "main");
        assert!(run(&[(W, &base)]).is_empty());
        // and the head under plain pull_request is not this finding
        let plain = text.replace("pull_request_target", "pull_request");
        assert!(run(&[(W, &plain)]).is_empty());
    }

    #[test]
    fn a_self_hosted_job_without_a_gpu_is_medium_and_with_one_is_not() {
        let text = "on: push\npermissions: {}\njobs:\n  build:\n    runs-on: [self-hosted, windows]\n    steps:\n      - run: cargo test\n";
        let found = run(&[(W, text)]);
        assert_eq!(subjects(&found), vec!["build:runner"]);
        assert_eq!(found[0].severity, Severity::Medium);
        let gpu = text.replace("[self-hosted, windows]", "[self-hosted, gpu]");
        assert!(run(&[(W, &gpu)]).is_empty());
        let by_step = text.replace("cargo test", "cargo test --features cuda");
        assert!(run(&[(W, &by_step)]).is_empty());
        let file = ".github/workflows/gpu-tests.yml";
        assert!(run(&[(file, text)]).is_empty());
        // an expression decides the runner: not judged
        let matrix = text.replace("[self-hosted, windows]", "${{ matrix.os }}");
        assert!(run(&[(W, &matrix)]).is_empty());
    }

    #[test]
    fn a_job_limited_to_other_events_does_not_run_for_a_pull_request() {
        let text = "on: [push, pull_request]
permissions: {}
jobs:
  publish:
    if: github.event_name == 'release'
    runs-on: ubuntu-latest
    steps:
      - run: x
        env:
          T: ${{ secrets.CARGO_REGISTRY_TOKEN }}
";
        assert!(run(&[(W, text)]).is_empty());
        // positive controls: an `||` that admits pull requests, and no condition at all, are still reported
        let or = text.replace(
            "== 'release'",
            "== 'release' || github.event_name == 'pull_request'",
        );
        assert_eq!(subjects(&run(&[(W, &or)])), vec!["publish:secrets"]);
        let by_ref = text.replace(
            "github.event_name == 'release'",
            "github.ref == 'refs/heads/main'",
        );
        assert!(run(&[(W, &by_ref)]).is_empty());
        let none = text.replace(
            "    if: github.event_name == 'release'
",
            "",
        );
        assert_eq!(subjects(&run(&[(W, &none)])), vec!["publish:secrets"]);
    }

    const SELF: &str = "on: pull_request
permissions: {}
jobs:
  dev:
    runs-on: [self-hosted, gpu]
    steps:
      - run: cargo test
";

    #[test]
    fn a_self_hosted_job_on_a_pull_request_needs_the_same_repo_condition_and_the_environment() {
        let found = run(&[(W, SELF)]);
        assert_eq!(subjects(&found), vec!["dev:same-repo"]);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(4));
        let cond = "    if: github.event.pull_request.head.repo.full_name == github.repository
";
        let gated = SELF.replace("    runs-on", &format!("{cond}    runs-on"));
        let found = run(&[(W, &gated)]);
        assert_eq!(subjects(&found), vec!["dev:environment"]);
        assert_eq!(found[0].severity, Severity::Medium);
        let both = gated.replace(
            "    runs-on",
            "    environment: gpu-runner
    runs-on",
        );
        assert!(run(&[(W, &both)]).is_empty());
        // positive controls: the reversed operand order is the same condition; a job that never runs for a pull request, a push
        // trigger and a hosted runner are not judged
        let reversed = both.replace(
            "github.event.pull_request.head.repo.full_name == github.repository",
            "github.repository == github.event.pull_request.head.repo.full_name",
        );
        assert!(run(&[(W, &reversed)]).is_empty());
        let release = SELF.replace(
            "    runs-on",
            "    if: github.event_name == 'release'
    runs-on",
        );
        assert!(run(&[(W, &release)]).is_empty());
        assert!(run(&[(W, &SELF.replace("on: pull_request", "on: push"))]).is_empty());
        assert!(run(&[(W, &SELF.replace("[self-hosted, gpu]", "ubuntu-latest"))]).is_empty());
        // a condition that names something else is not the same-repo condition
        let other = SELF.replace(
            "    runs-on",
            "    if: github.actor == 'me'
    runs-on",
        );
        assert_eq!(subjects(&run(&[(W, &other)])), vec!["dev:same-repo"]);
    }

    #[test]
    fn a_branch_filter_that_leaves_out_the_default_branch_is_medium() {
        let filtered = |filter: &str| {
            format!(
                "on:
  pull_request:
{filter}permissions: {{}}
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: x
"
            )
        };
        let found = run(&[(
            W,
            &filtered(
                "    branches: [develop]
",
            ),
        )]);
        assert_eq!(subjects(&found), vec!["on:branches"]);
        assert_eq!(found[0].severity, Severity::Medium);
        assert_eq!(found[0].location.as_ref().unwrap().line, Some(2));
        let ignored = run(&[(
            W,
            &filtered(
                "    branches-ignore: [main]
",
            ),
        )]);
        assert_eq!(subjects(&ignored), vec!["on:branches"]);
        // positive controls: the default branch listed, a catch-all, an unrelated ignore and no filter at all
        for ok in [
            "    branches: [main, develop]
",
            "    branches: ['**']
",
            "    branches-ignore: [wip]
",
            "    types: [opened]
",
        ] {
            assert!(run(&[(W, &filtered(ok))]).is_empty(), "{ok}");
        }
    }

    #[test]
    fn a_workflow_without_permissions_is_low() {
        let text =
            "on: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: x\n";
        let found = run(&[(W, text)]);
        assert_eq!(subjects(&found), vec!["permissions"]);
        assert_eq!(found[0].severity, Severity::Low);
    }

    #[test]
    fn a_file_that_is_not_a_workflow_is_named_in_a_note() {
        match judge(&RepoView::from_files(&[(W, "- just\n- a list\n")])) {
            Verdict::Noted(found, notes) => {
                assert!(found.is_empty());
                assert!(
                    notes.iter().any(|n| n.contains("does not parse")),
                    "{notes:?}"
                );
            }
            _ => panic!("expected notes"),
        }
    }
}
