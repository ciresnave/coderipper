// SPDX-License-Identifier: MIT OR Apache-2.0
//! SEC-009 (least privilege in infrastructure definitions), delegated to [checkov](https://github.com/bridgecrewio/checkov)
//! (Apache-2.0), run from the standalone single-file build its releases ship (no Python on the machine is needed).
//!
//! **What this judges:** checkov is given the project's *tracked* infrastructure files (`git ls-files`, never a walk of the disk):
//! Terraform (`.tf`, `.tf.json`), Dockerfiles, and the YAML or JSON files that Kubernetes, CloudFormation, ARM, Bicep and
//! Serverless definitions are written in. It is told which of its thousands of checks to run (`--check`): the ones in [`CHECKS`],
//! which are this rule's wording, **wildcard permissions and elevated container privileges** (a policy that allows `*` actions or
//! the whole `*:*`, an administrator grant, privilege escalation, a privileged container, a shared host namespace, an added Linux
//! capability, a container that runs as root). The rest of what checkov knows (encryption, logging, versioning, network exposure) is
//! other rules' business and is not reported here. The list is curated by hand against `checkov --list`; adding a check to it is a
//! reviewed change, and a check id that checkov stops knowing simply runs nothing.
//!
//! **Offline:** `--skip-download` (no external Terraform modules or policies are fetched) and `BC_SKIP_MAPPING=TRUE` (checkov's
//! startup download of its Bridgecrew check mapping is off). The tool runs in an empty scratch directory with a scrubbed
//! environment, so a `.checkov.yaml` in the project is not read; it is given no API key and uploads nothing.
//!
//! **Not judged is not clean:** a file checkov could not parse (`parsing_errors` in its report) beside an exit code of 0 or 1 is a
//! partial read: a clean result becomes a **coverage gap**, and findings carry a note. A project with no tracked infrastructure
//! file is a gap too: there was nothing to judge.
//!
//! **The repository cannot silence the rule:** checkov honours an inline `#checkov:skip=CKV_...:reason` comment (and the
//! `checkov.io/skip` annotation in Kubernetes). Its report lists such a check as skipped with the reason, and a skipped check of
//! this rule's list is reported anyway, flagged as suppressed in the repository. The project's way to suppress a finding is the
//! `.coderipper.toml` allowlist, which is visible.
//!
//! **The mapping record:** one finding per failed check, at the file and first line of the resource it names. Severity is by
//! class (see [`severity_of`]): a wildcard grant, an administrator policy, a privileged container, a shared host namespace or a
//! privilege escalation is `High`; a container that runs as root or adds a Linux capability is `Medium`. Confidence is `High`:
//! each check is a syntactic test of the definition. The text of the definition is third-party text that reaches a terminal:
//! control characters are removed and the length is cut.

use super::osv::{clean, relative, roots_of};
use super::zizmor::chunks;
use super::{resolve, run_tool_with_env, tracked_listing, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::Path;

/// The catalog rule this evidences.
pub(super) const RULE: &str = crate::rule_id::SEC_009;
/// The tool's name in the lock.
pub(super) const TOOL: &str = "checkov";

/// The checkov checks that are this rule's wording, and the only ones run. High: a wildcard or administrator grant, privilege
/// escalation, a privileged container, a shared host namespace. Medium: a container that runs as root or adds a capability.
const CHECKS: &[(&str, Severity)] = &[
    // wildcard and administrator grants
    ("CKV_AWS_1", Severity::High),
    ("CKV_AWS_49", Severity::High),
    ("CKV_AWS_62", Severity::High),
    ("CKV_AWS_63", Severity::High),
    ("CKV_AWS_72", Severity::High),
    ("CKV_AWS_274", Severity::High),
    ("CKV_AWS_275", Severity::High),
    ("CKV_AWS_355", Severity::High),
    ("CKV2_AWS_40", Severity::High),
    ("CKV_GCP_42", Severity::High),
    ("CKV_K8S_49", Severity::High),
    // privilege escalation
    ("CKV_AWS_110", Severity::High),
    ("CKV_AWS_286", Severity::High),
    ("CKV2_K8S_1", Severity::High),
    ("CKV2_K8S_2", Severity::High),
    ("CKV_K8S_20", Severity::High),
    ("CKV_K8S_5", Severity::High),
    // privileged containers and shared host namespaces
    ("CKV_K8S_2", Severity::High),
    ("CKV_K8S_16", Severity::High),
    ("CKV_K8S_159", Severity::High),
    ("CKV_K8S_3", Severity::High),
    ("CKV_K8S_18", Severity::High),
    ("CKV_K8S_4", Severity::High),
    ("CKV_K8S_19", Severity::High),
    ("CKV_AWS_210", Severity::High),
    ("CKV_AWS_316", Severity::High),
    ("CKV_AWS_334", Severity::High),
    // root containers and added capabilities
    ("CKV_K8S_6", Severity::Medium),
    ("CKV_K8S_23", Severity::Medium),
    ("CKV_K8S_24", Severity::Medium),
    ("CKV_K8S_25", Severity::Medium),
    ("CKV_K8S_28", Severity::Medium),
    ("CKV_K8S_7", Severity::Medium),
    ("CKV_K8S_36", Severity::Medium),
    ("CKV_K8S_37", Severity::Medium),
    ("CKV_K8S_39", Severity::Medium),
    ("CKV_DOCKER_8", Severity::Medium),
];

/// The frameworks checkov is asked to load. `secrets` (SEC-002's business), `github_actions` (SUP-008's) and `sca` (SUP-002's)
/// are other rules' and are not enabled.
const FRAMEWORKS: &[&str] = &[
    "terraform",
    "cloudformation",
    "kubernetes",
    "dockerfile",
    "arm",
    "bicep",
    "serverless",
];

/// A tracked file larger than this is not given to checkov: a definition is small, and a lockfile or a data dump is not one.
const MAX_INPUT_BYTES: u64 = 2 * 1024 * 1024;
/// Directories of other people's files, which are not the project's to fix: a tracked file below one is not read.
const SKIPPED_DIRS: &[&str] = &["node_modules", "vendor", "third_party", "target", ".git"];
/// Files named like a definition that are not one.
const SKIPPED_NAMES: &[&str] = &[
    "package.json",
    "package-lock.json",
    "composer.json",
    "composer.lock",
    "tsconfig.json",
    "deno.json",
    "renovate.json",
    "dependabot.yml",
    "dependabot.yaml",
    "codecov.yml",
    "mkdocs.yml",
];

/// Whether the project-relative path (with `/`) may hold an infrastructure definition.
fn is_input(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    if segments
        .iter()
        .any(|s| SKIPPED_DIRS.iter().any(|d| s.eq_ignore_ascii_case(d)))
    {
        return false;
    }
    // workflows are SUP-008's, not infrastructure
    if segments.starts_with(&[".github"]) {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    if SKIPPED_NAMES.contains(&lower.as_str()) {
        return false;
    }
    lower.ends_with(".tf")
        || lower.ends_with(".tf.json")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.ends_with(".json")
        || lower.ends_with(".template")
        || lower.ends_with(".bicep")
        || lower == "dockerfile"
        || lower.starts_with("dockerfile.")
        || lower.ends_with(".dockerfile")
}

/// The tracked definitions of the repository at `root` that exist on disk and are small enough, as absolute paths in the
/// listing's order.
fn select_inputs(listing: &str, root: &Path) -> Vec<OsString> {
    listing
        .split('\0')
        .filter(|p| !p.is_empty() && is_input(p))
        .map(|p| root.join(p))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_INPUT_BYTES))
        .map(std::path::PathBuf::into_os_string)
        .collect()
}

#[derive(Debug, Deserialize)]
struct Report {
    /// Absent from the summary-only object checkov prints when no check it was asked for applied to any file.
    results: Option<Results>,
    /// Present in that summary-only object (and in each framework's `summary`): what marks it as checkov's own.
    #[serde(default)]
    checkov_version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Results {
    #[serde(default)]
    failed_checks: Vec<Check>,
    #[serde(default)]
    skipped_checks: Vec<Check>,
    /// The files checkov could not parse.
    #[serde(default)]
    parsing_errors: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Check {
    check_id: String,
    #[serde(default)]
    check_name: String,
    file_abs_path: String,
    #[serde(default)]
    file_line_range: Vec<u32>,
    #[serde(default)]
    resource: String,
    #[serde(default)]
    check_result: CheckResult,
}

#[derive(Debug, Default, Deserialize)]
struct CheckResult {
    #[serde(default)]
    suppress_comment: Option<String>,
}

/// One checkov finding of a check this rule runs.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    check: String,
    name: String,
    severity: Severity,
    /// The file as checkov printed it (absolute) until [`relative`] makes it the project's.
    file: String,
    /// 1-based; the first line of the resource.
    line: u32,
    resource: String,
    /// The reason in the `#checkov:skip` comment that suppressed it, when one did.
    suppressed: Option<String>,
}

fn severity_of(id: &str) -> Option<Severity> {
    CHECKS.iter().find(|(c, _)| *c == id).map(|(_, s)| *s)
}

/// What a report came to: the hits of this rule's checks and how many files checkov could not parse.
struct Read {
    hits: Vec<Hit>,
    unparsed: usize,
}

/// Reads checkov's JSON report (`--output json`): one object per framework that found files, or a bare object when only one did.
/// A report that is not checkov's, or a check that cannot be placed in a file, is unreadable (a partial list must not look
/// like a complete one). Checks outside [`CHECKS`] are dropped.
fn parse_report(text: &str) -> Result<Read, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        format!(
            "the report is not a checkov result ({e}): {}",
            crate::redact::scrub(text)
                .chars()
                .take(200)
                .collect::<String>()
        )
    })?;
    let reports: Vec<Report> = match value {
        serde_json::Value::Array(_) => serde_json::from_value(value),
        serde_json::Value::Object(_) => serde_json::from_value(value).map(|r| vec![r]),
        _ => return Err("the report is neither a list nor an object".into()),
    }
    .map_err(|e| format!("the report is not a checkov result ({e})"))?;
    // an object with neither `results` nor `checkov_version` is not checkov's: a partial list must not look like a complete one
    if reports
        .iter()
        .any(|r| r.results.is_none() && r.checkov_version.is_none())
    {
        return Err("the report has no `results` and no `checkov_version`".into());
    }
    let (mut hits, mut unparsed) = (Vec::new(), 0);
    for report in reports {
        let results = report.results.unwrap_or_default();
        unparsed += results.parsing_errors.len();
        let failed = results.failed_checks.into_iter().map(|c| (c, false));
        let skipped = results.skipped_checks.into_iter().map(|c| (c, true));
        for (check, was_skipped) in failed.chain(skipped) {
            let Some(severity) = severity_of(&check.check_id) else {
                continue;
            };
            let line = *check
                .file_line_range
                .first()
                .ok_or_else(|| format!("a {} finding has no line range", check.check_id))?;
            hits.push(Hit {
                severity,
                name: check.check_name,
                file: check.file_abs_path,
                line,
                resource: check.resource,
                suppressed: was_skipped.then(|| {
                    check
                        .check_result
                        .suppress_comment
                        .unwrap_or_else(|| "no reason given".into())
                }),
                check: check.check_id,
            });
        }
    }
    hits.sort_by(|a, b| {
        (a.file.as_str(), a.line, &a.check).cmp(&(b.file.as_str(), b.line, &b.check))
    });
    Ok(Read { hits, unparsed })
}

/// The finding for one checkov finding. `roots` are the names the project root can be printed as.
fn to_finding(hit: &Hit, project: &str, roots: &[String], extra: Option<&str>) -> Finding {
    let file = clean(&relative(&hit.file, roots), 200);
    let resource = clean(&hit.resource, 200);
    let name = clean(&hit.name, 200);
    let summary = format!("{resource}: {name}");
    let mut detail = format!(
        "checkov ({}) reports {file} line {}: {name}. Grant only the named actions and resources the workload needs, and drop \
         container privileges and capabilities.",
        hit.check, hit.line
    );
    if let Some(reason) = &hit.suppressed {
        detail.push_str(&format!(
            " The definition carries a `checkov:skip` comment ({}), which CodeRipper does not honour (the project's way to suppress \
             a finding is the .coderipper.toml allowlist).",
            clean(reason, 120)
        ));
    }
    if let Some(extra) = extra {
        detail.push(' ');
        detail.push_str(extra);
    }
    let finding = Finding::new(
        RULE,
        hit.severity,
        Confidence::High,
        project,
        summary,
        detail,
    )
    .location(Location::new(file.clone(), Some(hit.line)))
    .subject(resource);
    // the summary quotes the tool's words, which can contain a whole word the host reads as an absence claim ("no policy ..."):
    // such a finding carries a control
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "checkov read {file} and reported {} at line {}",
            hit.check, hit.line
        )),
    }
}

/// The arguments of one run over `inputs` (absolute paths): offline, only [`CHECKS`] in the frameworks of [`FRAMEWORKS`], one JSON
/// document on stdout without code blocks.
fn check_args(inputs: &[OsString]) -> Vec<OsString> {
    let ids: Vec<&str> = CHECKS.iter().map(|(c, _)| *c).collect();
    let mut args: Vec<OsString> = [
        "--output",
        "json",
        "--compact",
        "--skip-download",
        "--framework",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.extend(FRAMEWORKS.iter().map(OsString::from));
    args.extend(["--check".into(), ids.join(",").into()]);
    for input in inputs {
        args.push("--file".into());
        args.push(input.clone());
    }
    args
}

/// What the whole project came to.
#[derive(Debug, Clone)]
enum Scan {
    Read {
        hits: Vec<Hit>,
        unparsed: usize,
    },
    /// Nothing was judged, and that is a gap, not a failure.
    Gap(String),
    Failed(ErrorKind, String),
}

/// Runs `program` over the project's tracked infrastructure files and reads what it prints.
fn scan(program: &Path, request: &Request) -> Scan {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and check nothing.
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    if !root.is_dir() {
        return Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{} is not a directory", root.display()),
        );
    }
    let inputs = match tracked_listing(&root) {
        Ok(listing) => select_inputs(&listing, &root),
        Err(why) => return Scan::Failed(ErrorKind::ToolFailed, why),
    };
    if inputs.is_empty() {
        return Scan::Gap(format!(
            "the project has no tracked infrastructure definition (Terraform, Dockerfile, Kubernetes, CloudFormation, ARM, Bicep or \
             Serverless), so there is nothing for {TOOL} to judge"
        ));
    }
    let (mut hits, mut unparsed) = (Vec::new(), 0);
    for group in chunks(&inputs) {
        let run = match run_tool_with_env(
            program,
            |_scratch| check_args(group),
            &request.limits,
            // checkov exits 1 when a check failed, which is an answer, not a failure
            &[1],
            &[("BC_SKIP_MAPPING", "TRUE")],
        ) {
            Ok(run) => run,
            Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
        };
        // no framework found a file it knows: checkov prints nothing, or a bare note, and exits 0
        let text = run.stdout();
        if text.trim().is_empty() {
            continue;
        }
        match parse_report(&text) {
            Ok(read) => {
                hits.extend(read.hits);
                unparsed += read.unparsed;
            }
            Err(why) => {
                return Scan::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}"))
            }
        }
    }
    if hits.len() > request.limits.max_findings {
        return Scan::Failed(
            ErrorKind::LimitExceeded,
            format!(
                "{TOOL} reported {} findings, past the limit of {}",
                hits.len(),
                request.limits.max_findings
            ),
        );
    }
    hits.sort_by(|a, b| {
        (a.file.as_str(), a.line, &a.check).cmp(&(b.file.as_str(), b.line, &b.check))
    });
    Scan::Read { hits, unparsed }
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    let program = match resolve(env, TOOL) {
        Resolved::Ready(program) => program,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"))
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    match scan(&program, request) {
        Scan::Read { hits, unparsed } => {
            let note = (unparsed > 0).then(|| {
                format!(
                    "Note: {TOOL} could not parse {unparsed} file(s); what it did not read was not judged."
                )
            });
            if hits.is_empty() {
                if let Some(note) = note {
                    return Verdict::Unavailable(format!(
                        "{TOOL} found nothing to report, but it cannot say the definitions are fine. {note}"
                    ));
                }
                return Verdict::Findings(vec![]);
            }
            let root = std::path::absolute(&request.project_root)
                .unwrap_or_else(|_| request.project_root.clone());
            let project = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let roots = roots_of(&root);
            Verdict::Findings(
                hits.iter()
                    .map(|h| to_finding(h, &project, &roots, note.as_deref()))
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

    /// What checkov 3.3.30 wrote (`--output json --compact`, measured 2026-10-25) for a Terraform file with a wildcard IAM policy
    /// and for a Kubernetes pod whose `privileged: true` is suppressed by an annotation; the fields this reads, with the ones it
    /// does not left out.
    const REPORT: &str = r#"[
      {"check_type":"terraform","results":{"passed_checks":[],"failed_checks":[
        {"check_id":"CKV_AWS_62","check_name":"Ensure IAM policies that allow full \"*-*\" administrative privileges are not created",
         "check_result":{"result":"FAILED"},"file_path":"\\iam.tf","file_abs_path":"/repo/iam.tf","file_line_range":[1,7],
         "resource":"aws_iam_policy.p","severity":null},
        {"check_id":"CKV_AWS_18","check_name":"Ensure the S3 bucket has access logging enabled",
         "check_result":{"result":"FAILED"},"file_abs_path":"/repo/iam.tf","file_line_range":[9,12],"resource":"aws_s3_bucket.b"}
      ],"skipped_checks":[],"parsing_errors":[]},"summary":{"failed":2}},
      {"check_type":"kubernetes","results":{"failed_checks":[],"skipped_checks":[
        {"check_id":"CKV_K8S_16","check_name":"Container should not be privileged",
         "check_result":{"result":"SKIPPED","suppress_comment":"needed for the driver"},
         "file_abs_path":"/repo/skip.yaml","file_line_range":[1,12],"resource":"Pod.default.q"}],
        "parsing_errors":["/repo/broken.yaml"]}}
    ]"#;

    #[test]
    fn only_the_checks_of_this_rule_are_kept_and_a_suppressed_one_is_flagged() {
        let read = parse_report(REPORT).unwrap();
        assert_eq!(read.unparsed, 1);
        let ids: Vec<&str> = read.hits.iter().map(|h| h.check.as_str()).collect();
        assert_eq!(
            ids,
            ["CKV_AWS_62", "CKV_K8S_16"],
            "CKV_AWS_18 is another rule's"
        );
        assert_eq!(read.hits[0].line, 1);
        assert_eq!(read.hits[0].severity, Severity::High);
        assert_eq!(read.hits[0].suppressed, None);
        assert_eq!(
            read.hits[1].suppressed.as_deref(),
            Some("needed for the driver")
        );
    }

    #[test]
    fn a_bare_object_is_one_framework() {
        let one = r#"{"check_type":"dockerfile","results":{"failed_checks":[
          {"check_id":"CKV_DOCKER_8","check_name":"Ensure the last USER is not root","file_abs_path":"/r/Dockerfile",
           "file_line_range":[2,2],"resource":"Dockerfile.USER"}]}}"#;
        let read = parse_report(one).unwrap();
        assert_eq!(read.hits.len(), 1);
        assert_eq!(read.hits[0].severity, Severity::Medium);
    }

    #[test]
    fn the_summary_only_object_checkov_prints_when_no_check_applied_is_a_clean_read() {
        let none = r#"{"passed":0,"failed":0,"skipped":0,"parsing_errors":0,"resource_count":0,"checkov_version":"3.3.30"}"#;
        let read = parse_report(none).unwrap();
        assert!(read.hits.is_empty() && read.unparsed == 0);
    }

    #[test]
    fn output_that_is_not_a_report_is_unreadable_never_clean() {
        assert!(parse_report("not json").is_err());
        assert!(parse_report("7").is_err());
        assert!(parse_report(r#"[{"nothing":1}]"#).is_err());
    }

    #[test]
    fn a_failed_check_with_no_line_is_unreadable() {
        let bad =
            r#"{"results":{"failed_checks":[{"check_id":"CKV_AWS_1","file_abs_path":"/r/a.tf"}]}}"#;
        assert!(parse_report(bad).is_err());
    }

    #[test]
    fn the_inputs_are_infrastructure_files_and_not_workflows_lockfiles_or_vendored_trees() {
        for yes in [
            "main.tf",
            "infra/net.tf.json",
            "deploy/pod.yaml",
            "Dockerfile",
            "docker/Dockerfile.prod",
            "app.dockerfile",
            "cfn/stack.template",
        ] {
            assert!(is_input(yes), "{yes}");
        }
        for no in [
            ".github/workflows/ci.yml",
            "package.json",
            "node_modules/x/main.tf",
            "vendor/y/Dockerfile",
            "README.md",
            "src/lib.rs",
        ] {
            assert!(!is_input(no), "{no}");
        }
    }

    #[test]
    fn every_check_is_listed_once_and_the_arguments_name_them_all() {
        let mut ids: Vec<&str> = CHECKS.iter().map(|(c, _)| *c).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "a check is listed twice");
        let args = check_args(&[OsString::from("/r/a.tf")]);
        let text: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(text.contains(&"--skip-download".to_string()));
        let list = &text[text.iter().position(|a| a == "--check").unwrap() + 1];
        assert_eq!(list.split(',').count(), before);
        assert_eq!(text.last().map(String::as_str), Some("/r/a.tf"));
    }

    #[test]
    fn the_finding_is_at_the_resource_and_says_when_a_skip_comment_was_not_honoured() {
        let hit = Hit {
            check: "CKV_K8S_16".into(),
            name: "Container should not be privileged".into(),
            severity: Severity::High,
            file: "/repo/skip.yaml".into(),
            line: 3,
            resource: "Pod.default.q".into(),
            suppressed: Some("needed for the driver".into()),
        };
        let f = to_finding(&hit, "repo", &["/repo".to_string()], None);
        assert_eq!(f.location.as_ref().unwrap().file, "skip.yaml");
        assert_eq!(f.location.as_ref().unwrap().line, Some(3));
        assert!(f.detail.contains("does not honour"), "{}", f.detail);
        assert!(f.summary.contains("Pod.default.q"));
    }
}
