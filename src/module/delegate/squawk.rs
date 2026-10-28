// SPDX-License-Identifier: MIT OR Apache-2.0
//! API-007 (database migrations are safe step by step), delegated to [squawk](https://github.com/sbdchd/squawk) (Apache-2.0), a
//! linter for PostgreSQL migrations, run from the single-file release build.
//!
//! **What this judges:** squawk is given the project's *tracked* migration files (`git ls-files`, never a walk of the disk): a `.sql`
//! file below a directory named `migrations`, `migration`, `migrate` or `db/migrate`, or a file named like a versioned migration
//! (`V12__name.sql`, `20260101120000_name.sql`). It reports the changes that take a table offline or lose data while a service is
//! running (a dropped column or table, a changed column type, a renamed column or table, a new `NOT NULL` column without a default,
//! a foreign key or unique constraint added without `NOT VALID`, an index built without `CONCURRENTLY`, ...). **The tool's style
//! advice is not reported as this rule** (`prefer-robust-stmts`, `prefer-bigint-over-int`, `prefer-identity`, `prefer-text-field`,
//! `prefer-timestamptz`, `require-lock-timeout`, `require-statement-timeout`): [`EXCLUDED`] is the list, and squawk refuses a rule name
//! it does not know, so a rename in a later release is an error, not a silently wider report.
//!
//! **Not judged is not clean:** squawk reads PostgreSQL. A file it cannot parse (another dialect, a template) is reported by squawk as
//! a `syntax-error` and is **not judged**: it is neither a finding nor evidence the migration is safe, so a clean result beside one is
//! a **coverage gap** and findings carry a note. A project with no migration file is a gap too.
//!
//! **The repository can silence the rule in one way, and the finding says when it has:** squawk honours `-- squawk-ignore <rule>`
//! comments in the SQL and has no switch to turn them off. Its configuration file (`.squawk.toml`) is **not** read: squawk is run in
//! an empty scratch directory with `--config` naming an empty file there. A migration that carries a `squawk-ignore` comment is noted
//! on every finding of the run, so a reader knows suppressions exist. The project's own way to suppress a finding is the
//! `.coderipper.toml` allowlist, which is visible.
//!
//! **The mapping record:** one finding per squawk finding, at the file and line it printed (squawk counts lines from 0). Severity by
//! rule: **High** for the changes that lose data or fail running code (`ban-drop-column`, `ban-drop-table`, `ban-drop-database`,
//! `ban-truncate-cascade`, `changing-column-type`, `renaming-column`, `renaming-table`, `adding-not-nullable-field`,
//! `adding-required-field`), **Medium** for the rest (locking and transaction advice). Confidence **Medium**: whether a table is large
//! enough for the lock to matter, or a column is read by running code, is not in the file. Text from the tool and the SQL is
//! control-character-cleaned and cut.

use super::osv::{clean, relative, roots_of};
use super::zizmor::chunks;
use super::{resolve, run_tool, tracked_listing, Resolved, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};
use crate::module::{ErrorKind, Request};
use crate::tools::ToolEnv;
use serde::Deserialize;
use std::ffi::OsString;
use std::path::Path;

/// The catalog rule this evidences.
pub(super) const RULE: &str = crate::rule_id::API_007;
/// The tool's name in the lock.
pub(super) const TOOL: &str = "squawk";

/// What squawk exits with when it found something (any other non-zero code is a failure of the tool).
const EXIT_FINDINGS: i32 = 1;
/// The style rules of squawk, which are not this rule's wording (see the module documentation).
const EXCLUDED: &[&str] = &[
    "prefer-robust-stmts",
    "prefer-bigint-over-int",
    "prefer-bigint-over-smallint",
    "prefer-identity",
    "prefer-text-field",
    "prefer-timestamptz",
    "require-lock-timeout",
    "require-statement-timeout",
];
/// The rules whose finding is High: a change that loses data or fails code that is running.
const HIGH: &[&str] = &[
    "ban-drop-column",
    "ban-drop-table",
    "ban-drop-database",
    "ban-truncate-cascade",
    "changing-column-type",
    "renaming-column",
    "renaming-table",
    "adding-not-nullable-field",
    "adding-required-field",
];
/// Directories of other people's files, which are not the project's to fix.
const SKIPPED_DIRS: &[&str] = &["node_modules", "vendor", "third_party", "target", ".git"];
/// A migration file larger than this is not given to squawk: it is a dump, not a migration.
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

/// Whether `name` is a versioned migration file name: `V12__add_users.sql` (flyway) or `20260101120000_add_users.sql`.
fn versioned_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".sql") {
        return false;
    }
    if let Some(rest) = name.strip_prefix(['V', 'v']) {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with("__") {
            return true;
        }
    }
    let digits = name.bytes().take_while(u8::is_ascii_digit).count();
    digits >= 8 && name[digits..].starts_with(['_', '-'])
}

/// Whether the project-relative path (with `/`) is a migration file.
fn is_input(path: &str) -> bool {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments.pop().unwrap_or_default();
    if segments
        .iter()
        .any(|s| SKIPPED_DIRS.iter().any(|d| s.eq_ignore_ascii_case(d)))
    {
        return false;
    }
    if !name.to_ascii_lowercase().ends_with(".sql") {
        return false;
    }
    segments.iter().any(|s| {
        ["migrations", "migration", "migrate", "migrates"]
            .iter()
            .any(|m| s.eq_ignore_ascii_case(m))
    }) || versioned_name(name)
}

/// The tracked migrations of the repository at `root` that exist on disk and are small enough, as absolute paths.
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
struct Entry {
    file: String,
    /// 0-based.
    line: u32,
    message: String,
    #[serde(default)]
    help: Option<String>,
    rule_name: String,
}

/// One squawk finding of a rule this record reports.
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    rule: String,
    file: String,
    /// 1-based.
    line: u32,
    message: String,
    help: Option<String>,
}

/// What a report came to: the hits, and how many entries were syntax errors (files not judged).
struct Read {
    hits: Vec<Hit>,
    unparsed: usize,
}

/// Reads squawk's JSON report (`--reporter json`): a list of entries. `syntax-error` entries are counted, not reported. A report
/// that is not squawk's is unreadable (a partial list must not look like a complete one).
fn parse_report(text: &str) -> Result<Read, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Read {
            hits: Vec::new(),
            unparsed: 0,
        });
    }
    let entries: Vec<Entry> = serde_json::from_str(trimmed).map_err(|e| {
        format!(
            "the report is not a squawk result ({e}): {}",
            crate::redact::scrub(trimmed)
                .chars()
                .take(200)
                .collect::<String>()
        )
    })?;
    let mut hits = Vec::new();
    let mut unparsed = 0;
    for e in entries {
        if e.rule_name == "syntax-error" {
            unparsed += 1;
            continue;
        }
        hits.push(Hit {
            rule: e.rule_name,
            file: e.file,
            line: e.line + 1,
            message: e.message,
            help: e.help,
        });
    }
    hits.sort_by(|a, b| {
        (a.file.as_str(), a.line, &a.rule).cmp(&(b.file.as_str(), b.line, &b.rule))
    });
    Ok(Read { hits, unparsed })
}

/// The finding for one squawk finding. `roots` are the names the project root can be printed as.
fn to_finding(hit: &Hit, project: &str, roots: &[String], extra: Option<&str>) -> Finding {
    let file = clean(&relative(&hit.file, roots), 200);
    let message = clean(&hit.message, 200);
    let severity = if HIGH.contains(&hit.rule.as_str()) {
        Severity::High
    } else {
        Severity::Medium
    };
    let mut detail = format!(
        "squawk ({}) reports {file} line {}: {message}.",
        clean(&hit.rule, 60),
        hit.line
    );
    if let Some(help) = &hit.help {
        detail.push(' ');
        detail.push_str(&clean(help, 240));
        detail.push('.');
    }
    detail.push_str(
        " Do it in steps that keep the old and the new code both working: add, copy, switch, then remove, and build indexes concurrently.",
    );
    if let Some(extra) = extra {
        detail.push(' ');
        detail.push_str(extra);
    }
    let finding = Finding::new(
        RULE,
        severity,
        Confidence::Medium,
        project,
        format!("{}: {message}", clean(&hit.rule, 60)),
        detail,
    )
    .location(Location::new(file.clone(), Some(hit.line)))
    .subject(clean(&hit.rule, 60));
    // the summary quotes the tool's words, which can hold a whole word the host reads as an absence claim ("no default value"):
    // such a finding carries a control
    match finding.clone().validate() {
        Ok(f) => f,
        Err(_) => finding.positive_control(format!(
            "squawk read {file} and reported {} at line {}",
            clean(&hit.rule, 60),
            hit.line
        )),
    }
}

/// The arguments of one run over `inputs`: JSON, the style rules off, the project's configuration replaced by an empty file.
fn check_args(inputs: &[OsString], config: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--reporter".into(),
        "json".into(),
        "--exclude".into(),
        EXCLUDED.join(",").into(),
        "--config".into(),
        config.as_os_str().to_owned(),
    ];
    args.extend(inputs.iter().cloned());
    args
}

/// How many `squawk-ignore` comments the migrations carry (squawk honours them and cannot be told not to).
fn ignore_comments(inputs: &[OsString]) -> usize {
    inputs
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().filter(|l| l.contains("squawk-ignore")).count())
        .sum()
}

/// What the whole project came to.
enum Scan {
    Read {
        hits: Vec<Hit>,
        unparsed: usize,
        ignores: usize,
    },
    Gap(String),
    Failed(ErrorKind, String),
}

fn scan(program: &Path, request: &Request) -> Scan {
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
            "the project has no tracked database migration (a .sql file in a migrations directory or named like a versioned migration), so there is nothing for {TOOL} to judge"
        ));
    }
    let (mut hits, mut unparsed) = (Vec::new(), 0);
    for group in chunks(&inputs) {
        let run = match run_tool(
            program,
            |scratch| {
                // an empty configuration beside the tool's working directory: the project's own `.squawk.toml` is never read
                let config = scratch.join("squawk.toml");
                let _ = std::fs::write(&config, "");
                check_args(group, &config)
            },
            &request.limits,
            &[EXIT_FINDINGS],
        ) {
            Ok(run) => run,
            Err((kind, detail)) => return Scan::Failed(kind, format!("{TOOL}: {detail}")),
        };
        match parse_report(&run.stdout()) {
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
    let ignores = ignore_comments(&inputs);
    Scan::Read {
        hits,
        unparsed,
        ignores,
    }
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
        Scan::Read {
            hits,
            unparsed,
            ignores,
        } => {
            let mut notes = Vec::new();
            if unparsed > 0 {
                notes.push(format!(
                    "Note: {TOOL} could not parse {unparsed} statement(s) (it reads PostgreSQL); what it did not read was not judged."
                ));
            }
            if ignores > 0 {
                notes.push(format!(
                    "Note: {ignores} `squawk-ignore` comment(s) in the migrations suppress findings, and squawk cannot be told to ignore them."
                ));
            }
            let note = (!notes.is_empty()).then(|| notes.join(" "));
            if hits.is_empty() {
                if unparsed > 0 {
                    return Verdict::Unavailable(format!(
                        "{TOOL} found nothing to report, but it cannot say the migrations are safe. {}",
                        note.unwrap_or_default()
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

    /// What squawk 2.70.0 wrote (`--reporter json`, measured 2026-10-26) for a migration with a `NOT NULL` column without a default and
    /// a file it could not parse; lines are 0-based.
    const REPORT: &str = r#"[
      {"file":"/r/migrations/1.sql","line":0,"column":18,"level":"Warning","message":"Adding a new column that is `NOT NULL` and has no default value to an existing table effectively makes it required.","help":"Make the field nullable or add a non-VOLATILE DEFAULT","rule_name":"adding-required-field","column_end":49,"line_end":0},
      {"file":"/r/migrations/1.sql","line":2,"column":0,"level":"Warning","message":"Dropping a table","help":null,"rule_name":"ban-drop-table","column_end":10,"line_end":2},
      {"file":"/r/migrations/bad.sql","line":0,"column":0,"level":"Error","message":"expected command, found IDENT","help":null,"rule_name":"syntax-error","column_end":0,"line_end":0}
    ]"#;

    #[test]
    fn findings_are_one_based_and_a_syntax_error_is_counted_not_reported() {
        let read = parse_report(REPORT).unwrap();
        assert_eq!(read.unparsed, 1);
        assert_eq!(read.hits.len(), 2);
        assert_eq!(
            (read.hits[0].rule.as_str(), read.hits[0].line),
            ("adding-required-field", 1)
        );
        assert_eq!(
            (read.hits[1].rule.as_str(), read.hits[1].line),
            ("ban-drop-table", 3)
        );
    }

    #[test]
    fn an_empty_report_is_clean_and_a_foreign_one_is_unreadable() {
        let clean_read = parse_report("[]").unwrap();
        assert!(clean_read.hits.is_empty() && clean_read.unparsed == 0);
        assert!(parse_report("").unwrap().hits.is_empty());
        assert!(parse_report("not json").is_err());
        assert!(parse_report(r#"{"a":1}"#).is_err());
        assert!(parse_report(r#"[{"nothing":1}]"#).is_err());
    }

    #[test]
    fn severity_follows_the_rule_and_the_finding_is_at_the_file_and_line() {
        let read = parse_report(REPORT).unwrap();
        let roots = vec!["/r".to_string()];
        let required = to_finding(&read.hits[0], "r", &roots, None);
        assert_eq!(required.severity, Severity::High);
        assert_eq!(required.location.as_ref().unwrap().file, "migrations/1.sql");
        assert_eq!(required.location.as_ref().unwrap().line, Some(1));
        let medium = to_finding(
            &Hit {
                rule: "constraint-missing-not-valid".into(),
                file: "/r/migrations/2.sql".into(),
                line: 4,
                message: "m".into(),
                help: None,
            },
            "r",
            &roots,
            Some("a note"),
        );
        assert_eq!(medium.severity, Severity::Medium);
        assert!(medium.detail.ends_with("a note"), "{}", medium.detail);
    }

    #[test]
    fn the_inputs_are_migrations_and_not_other_sql_or_vendored_trees() {
        for yes in [
            "migrations/001_init.sql",
            "db/migrate/20260101_add.sql",
            "app/Migrations/x.sql",
            "V12__add_users.sql",
            "sql/20260101120000_add_users.sql",
        ] {
            assert!(is_input(yes), "{yes}");
        }
        for no in [
            "schema.sql",
            "queries/report.sql",
            "node_modules/x/migrations/1.sql",
            "vendor/a/V1__b.sql",
            "migrations/readme.md",
            "V12_nope.sql",
        ] {
            assert!(!is_input(no), "{no}");
        }
    }

    #[test]
    fn the_excluded_rules_are_listed_once_and_passed_to_the_tool() {
        let mut names = EXCLUDED.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), EXCLUDED.len());
        let args = check_args(
            &[OsString::from("/r/migrations/1.sql")],
            Path::new("/s/squawk.toml"),
        );
        let text: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let list = &text[text.iter().position(|a| a == "--exclude").unwrap() + 1];
        assert_eq!(list.split(',').count(), EXCLUDED.len());
        assert!(text.contains(&"--config".to_string()));
        assert_eq!(text.last().map(String::as_str), Some("/r/migrations/1.sql"));
    }
}
