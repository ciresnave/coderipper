//! DOC-010 (documentation links resolve), delegated to [lychee](https://github.com/lycheeverse/lychee) (Apache-2.0 OR MIT).
//!
//! **What this judges:** lychee is given the project's *tracked* Markdown, HTML and text documents (`git ls-files`, never a walk of
//! the disk, so a virtualenv, a build directory or another worktree is not read; a tracked file below `node_modules`, `vendor`,
//! `third_party` or `target` is left out too), extracts their links and checks each: a link to a file or directory is looked for on
//! disk (a root-relative `/docs/a.md` is resolved against the project root, the reading GitHub uses; a site served from a
//! subdirectory reads them differently), a link to a web page is requested. Only a link that is
//! **known to be dead** is a finding: a local target that does not exist, or a web page that answered `404 Not Found` or
//! `410 Gone`. Anything else lychee could not settle (a timeout, a connection that failed, a rate limit, a `403` from a site that
//! turns bots away, a `5xx`) is *not judged*: it is neither a finding nor evidence of a working link.
//!
//! **Not judged is not clean.** A project with no links at all, a run where some links could not be settled, and a run where lychee
//! skipped a document it could not read (it exits 0 and says so only on stderr: measured with a file that is not UTF-8) are all
//! **coverage gaps**, never a clean rule. When dead links are found beside links that were not judged, each finding says so.
//!
//! **The repository cannot silence the rule:** lychee reads a `lychee.toml` (measured: a committed `exclude = [".*"]` makes it exit 0
//! having excluded every link), a `.lycheeignore` in its working directory and `.gitignore`. `lychee.toml` and `.lycheeignore` are
//! only looked for in the working directory (measured: from the project they silence the check, from another directory they do not),
//! and lychee is run from an empty scratch directory, so neither is read (the end-to-end test fails if the directory is the project's);
//! `--config` naming an empty file is a second guard no test exercises, and `--no-ignore` is kept so no ignore rule is ever applied
//! to the documents it is given. The project's way to suppress a finding is the `.coderipper.toml` allowlist, which is visible.
//!
//! **Narrower than the rule's wording:** a `#fragment` is not checked (the anchors a renderer generates differ from the checker's),
//! mail addresses and other schemes lychee does not support are not checked, and a link that only exists in a code block is not
//! extracted. The record says the same in `checked_today`.
//!
//! **The mapping record:** one finding per dead link occurrence, at the document and line it appears on, subject the link's target
//! (project-relative for a local one). Severity `Low` (the catalog default). Confidence `High` for a local target that does not
//! exist, `Medium` for a web page that answered 404 or 410 (a site can answer a bot differently from a reader). Link text is
//! third-party text that reaches a terminal: control characters are removed and the length is cut.

use super::osv::{clean, relative, roots_of};
use super::{resolve, run_tool, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

/// The catalog rule this evidences.
pub(super) const RULE: &str = "DOC-010";
/// The tool's name in the lock.
pub(super) const TOOL: &str = "lychee";

/// What lychee exits with besides 0: some link failed. (1 is an unexpected failure, such as an input it could not parse, and 3 a
/// configuration error; both are failures of the tool.)
const EXIT_LINKS: i32 = 2;
/// How long one request may take, and how often a failed one is retried. A link that needs longer is not judged.
const REQUEST_TIMEOUT_SECS: &str = "20";
const MAX_RETRIES: &str = "1";
/// Directories of other people's documents, which are not the project's to fix: a tracked file below one is not read.
const SKIPPED_DIRS: &[&str] = &["node_modules", "vendor", "third_party", "target", ".git"];
/// The extensions of the documents lychee reads (it knows these as Markdown, HTML or plain text).
const DOC_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "html", "htm", "txt"];

/// Whether the project-relative path (with `/`) is a document to check.
fn is_document(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    if segments
        .iter()
        .any(|s| SKIPPED_DIRS.iter().any(|d| s.eq_ignore_ascii_case(d)))
    {
        return false;
    }
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| DOC_EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// `text` with the characters lychee reads as glob patterns made literal (`[` `*` `?`): its inputs are globs, and a project in a
/// directory called `proj[12]` would otherwise match nothing (measured).
fn glob_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '[' | '*' | '?' => format!("[{c}]"),
            c => c.to_string(),
        })
        .collect()
}

/// The absolute, glob-escaped paths of the tracked documents in `listing` (the output of `git ls-files -z`) that exist on disk.
fn select_documents(listing: &str, root: &Path) -> Vec<String> {
    let base = root.to_string_lossy().replace('\\', "/");
    let base = glob_escape(base.trim_end_matches('/'));
    listing
        .split('\0')
        .filter(|p| !p.is_empty() && is_document(p) && root.join(p).is_file())
        .map(|p| format!("{base}/{}", glob_escape(p)))
        .collect()
}

/// The tracked documents of the repository at `root`, from the index.
fn tracked_documents(root: &Path) -> Result<Vec<String>, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .map_err(|e| format!("cannot run git to list the tracked files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git could not list the tracked files: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(select_documents(
        &String::from_utf8_lossy(&out.stdout),
        root,
    ))
}

#[derive(Debug, Deserialize)]
struct Report {
    total: u64,
    #[serde(default)]
    errors: u64,
    #[serde(default)]
    timeouts: u64,
    #[serde(default)]
    unknown: u64,
    error_map: BTreeMap<String, Vec<Entry>>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    url: String,
    status: Status,
    #[serde(default)]
    span: Option<Span>,
}

#[derive(Debug, Deserialize)]
struct Status {
    #[serde(default)]
    text: String,
    #[serde(default)]
    code: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct Span {
    line: u32,
}

/// One link known to be dead.
#[derive(Debug, Clone, PartialEq)]
struct Dead {
    /// The document, as lychee printed it until [`relative`] makes it the project's.
    file: String,
    line: Option<u32>,
    /// The target as lychee resolved it (a `file://` URL for a local one).
    url: String,
    /// What lychee said (`File not found...`, `Rejected status code: 404 Not Found`).
    why: String,
    local: bool,
}

/// What lychee read.
#[derive(Debug, Clone, PartialEq)]
struct Links {
    /// How many links it saw.
    total: u64,
    dead: Vec<Dead>,
    /// Links it could not settle (timeouts, failed connections, rate limits, other answers): not judged either way.
    unjudged: u64,
    /// What lychee reported when it could not read an input at all (its report then holds an entry whose "url" is the error).
    unread_input: Option<String>,
}

/// Whether the error is a local target that is not there (the only local error that means "dead").
fn local_missing(text: &str) -> bool {
    text.starts_with("File not found")
}

/// Reads lychee's JSON report (`--format json`). A report without `total` and `error_map` is not lychee's.
fn parse_report(text: &str) -> Result<Links, String> {
    let report: Report = serde_json::from_str(text).map_err(|e| {
        format!(
            "the report is not a lychee result ({e}): {}",
            text.chars().take(200).collect::<String>()
        )
    })?;
    let listed: u64 = report.error_map.values().map(|v| v.len() as u64).sum();
    if listed != report.errors {
        return Err(format!(
            "the report counts {} errors and lists {listed}",
            report.errors
        ));
    }
    let mut dead = Vec::new();
    let mut other_errors = 0;
    let mut unread_input = None;
    for (file, entries) in report.error_map {
        for entry in entries {
            // measured (a file the user may not read): the entry is not a link, its "url" is lychee's own error
            if entry.url.starts_with("error:") {
                unread_input.get_or_insert(entry.url);
                continue;
            }
            let local = entry.url.starts_with("file:");
            let is_dead = if local {
                local_missing(&entry.status.text)
            } else {
                matches!(entry.status.code, Some(404 | 410))
            };
            if is_dead {
                dead.push(Dead {
                    file: file.clone(),
                    line: entry.span.map(|s| s.line),
                    url: entry.url,
                    why: entry.status.text,
                    local,
                });
            } else {
                other_errors += 1;
            }
        }
    }
    // lychee reports in the order its requests finish: the findings are in the order of the documents
    dead.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    Ok(Links {
        total: report.total,
        dead,
        unjudged: other_errors + report.timeouts + report.unknown,
        unread_input,
    })
}

/// `text` with its `%XX` escapes decoded (lychee prints a `file://` URL encoded: a space is `%20`).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A `file://` URL as a path, without its query and fragment and with escapes decoded: `file:///C:/x/a%20b.md#top` is
/// `C:/x/a b.md`, and `file:///home/x` is `/home/x`.
fn file_url_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.split(['#', '?']).next().unwrap_or_default();
    let bytes = rest.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    Some(percent_decode(if drive { &rest[1..] } else { rest }))
}

/// A web address without the credentials some links carry (`https://user:token@host/` is `https://host/`): a finding must not
/// repeat a secret from a document.
fn without_credentials(url: &str) -> String {
    if let Some(at) = url.find("://") {
        let (scheme, rest) = url.split_at(at + 3);
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if let Some(i) = authority.rfind('@') {
            return format!("{scheme}{}{tail}", &authority[i + 1..]);
        }
    }
    url.to_string()
}

/// The finding for one dead link. `roots` are the names the project root can be printed as.
fn to_finding(dead: &Dead, project: &str, roots: &[String], extra: Option<&str>) -> Finding {
    let file = relative(&dead.file, roots);
    let target = if dead.local {
        file_url_path(&dead.url).map_or_else(|| dead.url.clone(), |p| relative(&p, roots))
    } else {
        without_credentials(&dead.url)
    };
    let target = clean(&target, 200);
    let document = clean(&file, 200);
    let (what, confidence) = if dead.local {
        ("does not exist", Confidence::High)
    } else {
        ("is dead", Confidence::Medium)
    };
    let mut detail = format!(
        "lychee followed the link to {target} from {document}{} and {}: {}.",
        dead.line.map_or(String::new(), |l| format!(" line {l}")),
        if dead.local {
            "found nothing there"
        } else {
            "the server answered that the page is gone"
        },
        clean(&dead.why, 120)
    );
    detail.push_str(" Fix the link or remove it.");
    if let Some(extra) = extra {
        detail.push(' ');
        detail.push_str(extra);
    }
    let finding = Finding::new(
        RULE,
        Severity::Low,
        confidence,
        project,
        format!("the link to {target} {what}"),
        detail,
    )
    .location(Location::new(file.clone(), dead.line))
    .subject(target.clone());
    // the target is third-party text and could be a whole word the host reads as an absence claim: such a finding carries a control
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "lychee read {file} and followed the link to {target}"
        )),
    }
}

/// The lines of stderr that say something went wrong. lychee prints `Hint:` lines beside link errors; every other line is a
/// document it skipped or an input it could not read (`[WARN] Skipping file with invalid UTF-8 content: ...`).
fn problem(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("Hint:"))
}

/// What the run came to.
#[derive(Debug, Clone)]
enum Scan {
    /// The links were read. `unread` is the first thing lychee said it could not read, when it said so.
    Read {
        links: Links,
        unread: Option<String>,
    },
    /// Nothing was judged, and that is a gap, not a failure.
    Gap(String),
    Failed(ErrorKind, String),
}

/// What the exit code, stderr and report add up to. The exit code says whether a link failed, stderr says whether everything was
/// read, and neither alone is the verdict.
fn judge(code: Option<i32>, stderr: &str, report: Result<Links, String>) -> Scan {
    match code {
        Some(code @ (0 | EXIT_LINKS)) => match report {
            Err(why) => Scan::Failed(ErrorKind::ToolOutputUnreadable, format!("{TOOL}: {why}")),
            Ok(links)
                if code == EXIT_LINKS
                    && links.dead.is_empty()
                    && links.unjudged == 0
                    && links.unread_input.is_none() =>
            {
                Scan::Failed(
                    ErrorKind::ToolOutputUnreadable,
                    format!("{TOOL} exited {EXIT_LINKS} (a link failed) but its report holds no failed link"),
                )
            }
            Ok(links)
                if code == 0
                    && (!links.dead.is_empty()
                        || links.unjudged > 0
                        || links.unread_input.is_some()) =>
            {
                Scan::Failed(
                    ErrorKind::ToolOutputUnreadable,
                    format!("{TOOL} exited 0 but its report holds failed links"),
                )
            }
            Ok(links) if links.total == 0 => match problem(stderr) {
                Some(why) => Scan::Failed(
                    ErrorKind::ToolFailed,
                    format!("{TOOL} found no links and could not read part of the project: {why}"),
                ),
                None => Scan::Gap(format!(
                    "{TOOL} found no links in the project's documents, so there is nothing for this rule to judge; it says nothing about \
                     documents {TOOL} does not read"
                )),
            },
            Ok(links) => Scan::Read {
                unread: problem(stderr)
                    .map(str::to_string)
                    .or_else(|| links.unread_input.clone()),
                links,
            },
        },
        Some(code) => Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} exited with code {code}: {stderr}"),
        ),
        None => Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{TOOL} ended without an exit code: {stderr}"),
        ),
    }
}

/// The configuration lychee is run with, written to the scratch directory: an empty one, so no project's `lychee.toml` can exclude
/// links or accept status codes.
const CONFIG: &str = "";

/// The arguments of the check of the project at `root`: the documents listed in `scratch/files.txt` (see [`tracked_documents`]),
/// JSON into `scratch`, the repository's own `lychee.toml` replaced, root-relative links resolved against `root`. `root` is absolute
/// (the tool runs in `scratch`, where a `.lycheeignore` of the project is not found).
fn check_args(root: &Path, scratch: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--no-progress",
        "--no-ignore",
        "--format",
        "json",
        "--timeout",
        REQUEST_TIMEOUT_SECS,
        "--max-retries",
        MAX_RETRIES,
    ]
    .iter()
    .map(OsString::from)
    .collect();
    for (flag, file) in [("--config", "config.toml"), ("--output", "report.json")] {
        args.push(flag.into());
        args.push(scratch.join(file).into());
    }
    args.push("--files-from".into());
    args.push(scratch.join("files.txt").into());
    args.push("--root-dir".into());
    args.push(root.into());
    args
}

/// Runs `program` with the arguments `args(root, scratch)` builds and reads the report it was told to write at
/// `scratch/report.json`. (The arguments are a parameter so a test can stand in a program that misbehaves; the tool is always run
/// with [`check_args`].)
fn scan_with(program: &Path, args: fn(&Path, &Path) -> Vec<OsString>, request: &Request) -> Scan {
    // The tool runs in a scratch directory, so a relative project path would be read from the wrong place and check nothing.
    let root =
        std::path::absolute(&request.project_root).unwrap_or_else(|_| request.project_root.clone());
    if !root.is_dir() {
        return Scan::Failed(
            ErrorKind::ToolFailed,
            format!("{} is not a directory", root.display()),
        );
    }
    let documents = match tracked_documents(&root) {
        Ok(documents) => documents,
        Err(why) => return Scan::Failed(ErrorKind::ToolFailed, why),
    };
    if documents.is_empty() {
        return Scan::Gap(format!(
            "the project has no tracked Markdown, HTML or text document, so there is nothing for {TOOL} to judge"
        ));
    }
    let run = match run_tool(
        program,
        |scratch| {
            // a failure to write these surfaces as the tool failing to start with them
            let _ = std::fs::write(scratch.join("config.toml"), CONFIG);
            let _ = std::fs::write(scratch.join("files.txt"), documents.join("\n"));
            args(&root, scratch)
        },
        &request.limits,
        &[EXIT_LINKS],
    ) {
        Ok(run) => run,
        Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
    };
    let report = std::fs::read(run.scratch().join("report.json"))
        .map_err(|e| format!("it wrote no report ({e})"))
        .and_then(|bytes| parse_report(&String::from_utf8_lossy(&bytes)));
    match judge(run.exit_code(), run.log(), report) {
        Scan::Read { links, unread } if links.dead.len() > request.limits.max_findings => {
            let _ = unread;
            Scan::Failed(
                ErrorKind::LimitExceeded,
                format!(
                    "{TOOL} reported {} dead links, past the limit of {}",
                    links.dead.len(),
                    request.limits.max_findings
                ),
            )
        }
        other => other,
    }
}

/// What a result that is not clean says about the links that were not judged.
fn caveat(unjudged: u64, unread: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if unjudged > 0 {
        parts.push(format!(
            "{unjudged} other link{} could not be settled (a timeout, a failed connection, a rate limit or another answer) and {} not judged",
            if unjudged == 1 { "" } else { "s" },
            if unjudged == 1 { "was" } else { "were" }
        ));
    }
    if let Some(why) = unread {
        parts.push(format!(
            "lychee could not read part of the project ({}); what it did not read was not judged",
            clean(why, 200)
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("Note: {}.", parts.join("; ")))
    }
}

pub(super) fn run(env: &ToolEnv, request: &Request) -> Verdict {
    if !request.network {
        return Verdict::Unavailable(format!(
            "{TOOL} requests web pages and this run does not permit the network (a sweep does)"
        ));
    }
    let program = match resolve(env, TOOL) {
        Resolved::Ready(program) => program,
        Resolved::Unavailable(why) => {
            return Verdict::Unavailable(format!("{TOOL} is not available: {why}"))
        }
        Resolved::Failed(kind, why) => return Verdict::Failed(kind, why),
    };
    match scan_with(&program, check_args, request) {
        Scan::Read { links, unread } => {
            if links.dead.is_empty() {
                if let Some(note) = caveat(links.unjudged, unread.as_deref()) {
                    return Verdict::Unavailable(format!(
                        "{TOOL} found no dead link, but it cannot say the links are fine. {note}"
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
            let note = caveat(links.unjudged, unread.as_deref());
            Verdict::Findings(
                links
                    .dead
                    .iter()
                    .map(|d| to_finding(d, &project, &roots, note.as_deref()))
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

    /// What lychee 0.24.2 wrote (`--format json`, measured 2026-10-09) for a project with a dead local link, a root-relative one,
    /// a 404, a 429, a connection that failed and a request that timed out (the unread maps and durations are left out).
    const REPORT: &str = r#"{
  "total": 9, "unique": 9, "successful": 2, "unknown": 0, "unsupported": 0, "timeouts": 1, "redirects": 0, "remaps": 0, "excludes": 1,
  "errors": 5, "cached": 0,
  "success_map": {},
  "error_map": {
    "C:/work/demo\\docs\\a.md": [
      { "url": "file:///C:/work/demo/zzz.md", "status": { "text": "File not found. Check if file exists and path is correct", "details": "File not found. Check if file exists and path is correct" }, "span": { "line": 3, "column": 1 } }
    ],
    "C:/work/demo\\README.md": [
      { "url": "file:///C:/work/demo/docs/nope.md", "status": { "text": "File not found. Check if file exists and path is correct", "details": "x" }, "span": { "line": 5, "column": 1 } },
      { "url": "https://github.com/o/gone/", "status": { "text": "Rejected status code: 404 Not Found", "code": 404 }, "span": { "line": 7, "column": 3 } },
      { "url": "https://httpbin.org/status/429", "status": { "text": "Rejected status code: 429 Too Many Requests", "code": 429 }, "span": { "line": 8, "column": 1 } },
      { "url": "http://zz9-no-such-host-abc.com/", "status": { "text": "Network error: Connection failed. Check network connectivity and firewall settings (error sending request)", "details": "Connection failed." }, "span": { "line": 9, "column": 1 } }
    ]
  },
  "timeout_map": { "C:/work/demo\\README.md": [ { "url": "http://10.255.255.1/", "status": { "text": "Timeout", "details": "Request timed out" }, "span": { "line": 10, "column": 1 } } ] },
  "excluded_map": {}, "duration": { "secs": 1, "nanos": 0 }, "detailed_stats": false
}"#;

    fn roots() -> Vec<String> {
        vec!["C:/work/demo".to_string()]
    }

    #[test]
    fn the_real_report_shape_gives_dead_links_and_counts_the_rest_as_not_judged() {
        let links = parse_report(REPORT).unwrap();
        assert_eq!(links.total, 9);
        let seen: Vec<_> = links
            .dead
            .iter()
            .map(|d| (relative(&d.file, &roots()), d.line, d.local))
            .collect();
        assert_eq!(
            seen,
            [
                ("README.md".to_string(), Some(5), true),
                ("README.md".to_string(), Some(7), false),
                ("docs/a.md".to_string(), Some(3), true),
            ]
        );
        // the 429, the failed connection and the timeout are not dead and not fine
        assert_eq!(links.unjudged, 3);
    }

    #[test]
    fn only_a_missing_local_target_or_a_404_or_410_is_dead() {
        let report = |entry: &str, errors: u32| {
            format!(r#"{{"total":1,"errors":{errors},"error_map":{{"a.md":[{entry}]}}}}"#)
        };
        let dead = |entry: &str| parse_report(&report(entry, 1)).unwrap().dead.len();
        for (entry, want) in [
            (
                r#"{"url":"https://x.test/","status":{"text":"t","code":410}}"#,
                1,
            ),
            (
                r#"{"url":"https://x.test/","status":{"text":"t","code":404}}"#,
                1,
            ),
            (
                r#"{"url":"https://x.test/","status":{"text":"t","code":403}}"#,
                0,
            ),
            (
                r#"{"url":"https://x.test/","status":{"text":"t","code":500}}"#,
                0,
            ),
            (
                r#"{"url":"https://x.test/","status":{"text":"Network error"}}"#,
                0,
            ),
            (
                r#"{"url":"file:///a/b","status":{"text":"File not found. x"}}"#,
                1,
            ),
            (
                r#"{"url":"file:///a/b","status":{"text":"Permission denied"}}"#,
                0,
            ),
        ] {
            assert_eq!(dead(entry), want, "{entry}");
        }
    }

    #[test]
    fn a_report_that_is_not_lychees_or_disagrees_with_itself_is_unreadable() {
        for bad in [
            "",
            "null",
            "[]",
            "{}",
            "not json",
            r#"{"total":1}"#,
            r#"{"error_map":{}}"#,
            r#"{"total":1,"errors":2,"error_map":{}}"#,
        ] {
            assert!(
                parse_report(bad).is_err(),
                "{bad:?} must be unreadable, not clean"
            );
        }
        assert_eq!(
            parse_report(r#"{"total":0,"error_map":{}}"#).unwrap().total,
            0
        );
    }

    #[test]
    fn file_urls_become_paths_on_both_platforms() {
        assert_eq!(
            file_url_path("file:///C:/work/x.md").as_deref(),
            Some("C:/work/x.md")
        );
        assert_eq!(
            file_url_path("file:///home/me/x.md").as_deref(),
            Some("/home/me/x.md")
        );
        assert_eq!(file_url_path("https://x.test/"), None);
    }

    #[test]
    fn a_finding_names_the_document_the_line_and_the_target_and_is_valid() {
        let links = parse_report(REPORT).unwrap();
        let local = to_finding(&links.dead[0], "demo", &roots(), None);
        assert_eq!(local.check_id, "DOC-010");
        assert_eq!(local.severity, Severity::Low);
        assert_eq!(local.confidence, Confidence::High);
        let location = local.location.as_ref().unwrap();
        assert_eq!(
            (location.file.as_str(), location.line),
            ("README.md", Some(5))
        );
        assert_eq!(local.subject.as_deref(), Some("docs/nope.md"));
        assert!(
            local.summary.contains("docs/nope.md") && local.summary.contains("does not exist"),
            "{}",
            local.summary
        );
        assert!(local.clone().validate().is_ok());
        let web = to_finding(&links.dead[1], "demo", &roots(), None);
        assert_eq!(web.confidence, Confidence::Medium);
        assert_eq!(web.subject.as_deref(), Some("https://github.com/o/gone/"));
        assert!(web.detail.contains("404"), "{}", web.detail);
        assert!(web.validate().is_ok());
    }

    #[test]
    fn link_text_cannot_carry_terminal_control_codes_or_run_on() {
        let mut dead = parse_report(REPORT).unwrap().dead[1].clone();
        dead.url = format!("https://x.test/\u{1b}[31m{}", "x".repeat(500));
        dead.why = format!("evil\u{1b}[31m{}", "y".repeat(500));
        let f = to_finding(&dead, "demo", &roots(), None);
        let text = format!("{} {} {:?}", f.summary, f.detail, f.subject);
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(f.detail.len() < 800, "{}", f.detail.len());
    }

    #[test]
    fn a_link_called_none_still_makes_a_valid_finding() {
        let mut dead = parse_report(REPORT).unwrap().dead[0].clone();
        dead.url = "file:///C:/work/demo/none".into(); // a whole word: the host would read the summary as an absence claim
        let f = to_finding(&dead, "demo", &roots(), None);
        assert!(f.positive_control.is_some());
        assert!(f.validate().is_ok());
    }

    #[test]
    fn the_run_ignores_whatever_the_repository_says_about_itself() {
        let args: Vec<String> = check_args(Path::new("/abs/repo"), Path::new("/scratch"))
            .iter()
            .map(|a| a.to_string_lossy().replace('\\', "/"))
            .collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert!(
            has("--no-ignore"),
            "a .gitignore must not hide a document: {args:?}"
        );
        assert_eq!(after("--format"), "json");
        assert_eq!(
            after("--config"),
            "/scratch/config.toml",
            "the repository's lychee.toml is never read"
        );
        assert_eq!(after("--output"), "/scratch/report.json");
        assert_eq!(after("--root-dir"), "/abs/repo");
        assert_eq!(
            after("--files-from"),
            "/scratch/files.txt",
            "only the tracked documents are read, never a walk of the disk"
        );
        assert!(!has("--hidden") && !has("--exclude-path"));
        for flag in [
            "--cache",
            "--include-verbatim",
            "--accept",
            "--exclude",
            "--offline",
        ] {
            assert!(!has(flag), "{flag} would change what is judged: {args:?}");
        }
    }

    #[test]
    fn only_tracked_documents_outside_other_peoples_directories_are_read() {
        for (path, want) in [
            ("README.md", true),
            ("docs/a.MD", true),
            (".github/CONTRIBUTING.md", true),
            ("site/page.html", true),
            ("notes.txt", true),
            ("src/main.rs", false),
            ("Makefile", false),
            ("node_modules/x/README.md", false),
            ("web/node_modules/x/README.md", false),
            ("vendor/lib/README.md", false),
            ("a/third_party/b.md", false),
            ("target/doc/index.html", false),
            ("docs/target.md", true),
            ("vendorish/a.md", true),
        ] {
            assert_eq!(is_document(path), want, "{path}");
        }
    }

    #[test]
    fn the_documents_are_listed_from_the_index_and_made_safe_as_glob_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pr[12] x");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("README.md"), "x").unwrap();
        std::fs::write(root.join("docs/a b.md"), "x").unwrap();
        // tracked but deleted from the working tree, and not a document
        let listing = "README.md\0docs/a b.md\0gone.md\0src/lib.rs\0\0";
        let found = select_documents(listing, &root);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|p| p.contains("pr[[]12] x/")), "{found:?}");
        assert!(found[1].ends_with("docs/a b.md"), "{found:?}");
        assert_eq!(glob_escape("a*b?c[d]"), "a[*]b[?]c[[]d]");
    }

    #[test]
    fn a_file_url_is_decoded_and_loses_its_query_and_fragment() {
        assert_eq!(
            file_url_path("file:///C:/work/sp%20ace/n%C3%B6.md#top").as_deref(),
            Some("C:/work/sp ace/nö.md")
        );
        assert_eq!(
            file_url_path("file:///home/x/a.md?x=1").as_deref(),
            Some("/home/x/a.md")
        );
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%2"), "%zz%2");
        let mut dead = parse_report(REPORT).unwrap().dead[0].clone();
        dead.url = "file:///C:/work/demo/sp%20ace/gone%20file.md#h".into();
        let f = to_finding(&dead, "demo", &roots(), None);
        assert_eq!(f.subject.as_deref(), Some("sp ace/gone file.md"));
    }

    #[test]
    fn credentials_in_a_link_are_not_repeated() {
        assert_eq!(
            without_credentials("https://user:tok@host.test/a?b=c@d"),
            "https://host.test/a?b=c@d"
        );
        assert_eq!(
            without_credentials("https://host.test/a@b"),
            "https://host.test/a@b"
        );
        let mut dead = parse_report(REPORT).unwrap().dead[1].clone();
        dead.url = "https://user:tok@host.test/x".into();
        let f = to_finding(&dead, "demo", &roots(), None);
        let text = format!("{} {} {:?}", f.summary, f.detail, f.subject);
        assert!(!text.contains("tok"), "{text}");
    }

    #[test]
    fn findings_come_in_document_and_line_order_whatever_order_lychee_finished_in() {
        let report = r#"{"total":3,"errors":3,"error_map":{"b.md":[
            {"url":"file:///x/3","status":{"text":"File not found. x"},"span":{"line":9}},
            {"url":"file:///x/1","status":{"text":"File not found. x"},"span":{"line":2}}],
            "a.md":[{"url":"file:///x/2","status":{"text":"File not found. x"},"span":{"line":5}}]}}"#;
        let lines: Vec<_> = parse_report(report)
            .unwrap()
            .dead
            .iter()
            .map(|d| (d.file.clone(), d.line))
            .collect();
        assert_eq!(
            lines,
            [
                ("a.md".to_string(), Some(5)),
                ("b.md".to_string(), Some(2)),
                ("b.md".to_string(), Some(9))
            ]
        );
    }

    #[test]
    fn an_input_lychee_could_not_read_is_named_as_that_not_as_a_link_that_timed_out() {
        // measured: with a file the user may not read, exit 2 and an entry whose "url" is lychee's own error
        let report = r#"{"total":1,"errors":1,"error_map":{"README.md":[
            {"url":"error: Error reading input 'x.md': Permission denied","status":{"text":"x"}}]}}"#;
        let links = parse_report(report).unwrap();
        assert_eq!((links.unjudged, links.dead.len()), (0, 0));
        assert!(links.unread_input.is_some());
        let scan = judge(Some(2), "", Ok(links));
        assert!(
            matches!(&scan, Scan::Read { unread: Some(why), .. } if why.contains("Permission denied")),
            "{scan:?}"
        );
    }

    fn links(dead: usize, unjudged: u64, total: u64) -> Result<Links, String> {
        let template = parse_report(REPORT).unwrap().dead[0].clone();
        Ok(Links {
            total,
            dead: vec![template; dead],
            unjudged,
            unread_input: None,
        })
    }

    #[test]
    fn exit_2_with_dead_links_is_findings_and_exit_0_with_none_is_clean() {
        assert!(
            matches!(judge(Some(2), "Hint: x", links(2, 0, 5)), Scan::Read { links, unread: None } if links.dead.len() == 2)
        );
        assert!(
            matches!(judge(Some(0), "", links(0, 0, 5)), Scan::Read { links, unread: None } if links.dead.is_empty() && links.unjudged == 0)
        );
    }

    #[test]
    fn hint_lines_beside_link_errors_are_not_a_partial_read() {
        let hints = "Hint: Encountered rate limit responses. x\nHint: You can configure accepted/rejected response codes with `-a`\n";
        assert!(matches!(
            judge(Some(2), hints, links(1, 1, 5)),
            Scan::Read { unread: None, .. }
        ));
    }

    #[test]
    fn a_document_lychee_skipped_is_a_partial_read_even_though_it_exited_0() {
        // measured: a file that is not UTF-8 is skipped with a warning on stderr and the exit code is still 0
        let warn = "  [WARN] Skipping file with invalid UTF-8 content: C:/x/bin.md";
        let scan = judge(Some(0), warn, links(0, 0, 3));
        assert!(
            matches!(&scan, Scan::Read { unread: Some(why), .. } if why.contains("invalid UTF-8")),
            "{scan:?}"
        );
        // and nothing read at all is a failure, not the "no links" gap
        assert!(
            matches!(judge(Some(0), warn, links(0, 0, 0)), Scan::Failed(ErrorKind::ToolFailed, why) if why.contains("invalid UTF-8"))
        );
    }

    #[test]
    fn no_links_is_a_gap_because_nothing_was_there_to_judge() {
        let scan = judge(Some(0), "", links(0, 0, 0));
        assert!(
            matches!(&scan, Scan::Gap(why) if why.contains("no links")),
            "{scan:?}"
        );
    }

    #[test]
    fn exit_codes_that_do_not_agree_with_the_report_or_are_not_lychees_answers_are_failures() {
        // exit 2 says a link failed; a report with none is not believed, and neither is exit 0 beside failures
        assert!(matches!(
            judge(Some(2), "", links(0, 0, 4)),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
        assert!(matches!(
            judge(Some(0), "", links(1, 0, 4)),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
        assert!(matches!(
            judge(Some(0), "", links(0, 1, 4)),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
        assert!(matches!(
            judge(Some(0), "", Err("not json".into())),
            Scan::Failed(ErrorKind::ToolOutputUnreadable, _)
        ));
        // 1 (an input it could not parse) and 3 (configuration) are the tool failing
        assert!(
            matches!(judge(Some(1), "Error: Cannot parse inputs", Err("none".into())), Scan::Failed(ErrorKind::ToolFailed, why) if why.contains("Cannot parse"))
        );
        assert!(matches!(
            judge(Some(3), "bad config", Err("none".into())),
            Scan::Failed(ErrorKind::ToolFailed, _)
        ));
        assert!(matches!(judge(None, "", links(0, 0, 4)), Scan::Failed(..)));
    }

    #[test]
    fn links_not_judged_are_said_in_every_finding_and_a_clean_result_needs_none() {
        assert_eq!(caveat(0, None), None);
        let note = caveat(3, None).unwrap();
        assert!(
            note.contains("3 other links") && note.contains("not judged"),
            "{note}"
        );
        assert!(caveat(1, None).unwrap().contains("1 other link could not"));
        let skipped = caveat(0, Some("[WARN] Skipping file x")).unwrap();
        assert!(skipped.contains("Skipping file x"), "{skipped}");
        let dead = parse_report(REPORT).unwrap().dead[0].clone();
        let f = to_finding(&dead, "demo", &roots(), Some(&note));
        assert!(f.detail.ends_with(&note), "{}", f.detail);
        assert!(f.validate().is_ok());
    }

    #[test]
    fn the_record_says_what_is_checked_today_and_what_is_not() {
        let catalog = crate::catalog::Catalog::builtin();
        let text = catalog
            .get(RULE)
            .unwrap()
            .checked_today()
            .expect("the record does not say what is checked today");
        assert!(text.contains("404") && text.contains("fragment"), "{text}");
        assert!(text.contains("not judged"), "{text}");
    }
}
