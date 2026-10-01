//! Runs `cargo build --all-targets --message-format=json` and parses rustc's diagnostics, keeping
//! the parts a check needs: the lint code, the primary span's line AND column, and the text of each
//! direct child note (where a `#[must_use = "..."]` reason string shows up).

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// Forward-slash path, relative to the directory cargo ran in.
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Diagnostic {
    pub code: Option<String>,
    pub level: String,
    pub message: String,
    /// `message` of each direct child (notes, helps) in order.
    pub notes: Vec<String>,
    pub spans: Vec<Span>,
}

impl Diagnostic {
    pub fn primary_span(&self) -> Option<&Span> {
        self.spans.iter().find(|s| s.is_primary)
    }

    /// An `error` that is not just a lint promoted by `#![deny(...)]`/`-D warnings`: rustc error
    /// codes look like `E0603`; lint names are lowercase snake_case; a syntax error has no code.
    pub fn is_hard_error(&self) -> bool {
        self.level == "error"
            && match &self.code {
                None => true,
                Some(code) => {
                    code.starts_with('E') && code[1..].chars().all(|c| c.is_ascii_digit())
                }
            }
    }
}

pub(crate) struct BuildOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub success: bool,
}

impl BuildOutput {
    /// True when this build can't be trusted to have reported what a check looked for: a real
    /// compiler error, or a non-zero exit with no diagnostic at all (e.g. a panicking build.rs,
    /// which writes plain text to stderr rather than rustc JSON).
    pub fn is_broken(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_hard_error)
            || (!self.success && self.diagnostics.is_empty())
    }
}

#[derive(Deserialize)]
struct CargoMessage {
    reason: String,
    message: Option<RawDiagnostic>,
}

#[derive(Deserialize)]
struct RawDiagnostic {
    code: Option<RawCode>,
    level: String,
    message: String,
    spans: Vec<RawSpan>,
    children: Vec<RawChild>,
}

#[derive(Deserialize)]
struct RawCode {
    code: String,
}

#[derive(Deserialize)]
struct RawChild {
    message: String,
}

#[derive(Deserialize)]
struct RawSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    is_primary: bool,
    /// Present when the span is inside macro-expanded code; `span` is the macro CALL site.
    expansion: Option<Box<RawExpansion>>,
}

#[derive(Deserialize)]
struct RawExpansion {
    span: RawSpan,
}

impl RawSpan {
    /// rustc reports a lint inside a `macro_rules!` body at the macro DEFINITION, once per
    /// invocation. Follow `expansion` out to the outermost call site, which is the only thing that
    /// tells two invocations apart (and is where `unused_must_use` already points).
    fn into_span(self) -> Span {
        let is_primary = self.is_primary;
        let mut cur = self;
        while let Some(exp) = cur.expansion.take() {
            cur = exp.span;
        }
        Span {
            file: cur.file_name.replace('\\', "/"),
            line: cur.line_start,
            column: cur.column_start,
            is_primary,
        }
    }
}

/// Extra rustc flags for the analysis build, appended to whatever the caller already set:
/// - `--cap-lints=warn`: a `#![deny(warnings)]`, `[lints]` table or `-D warnings` must not turn the
///   tags into errors, or cargo never compiles the crates that depend on the failed one and their
///   uses silently go uncounted.
/// - `--force-warn <lint>`: an `allow` (crate-wide or on one module) must not silence the two lints
///   this check counts, or uses inside it silently go uncounted.
const ANALYSIS_RUSTFLAGS: &str =
    "--cap-lints=warn --force-warn deprecated --force-warn unused_must_use";

pub(crate) fn build_all_targets(root: &Path) -> anyhow::Result<BuildOutput> {
    let existing = std::env::var("RUSTFLAGS").unwrap_or_default();
    let output = Command::new("cargo")
        .args(["build", "--all-targets", "--message-format=json"])
        .current_dir(root)
        .env("RUSTFLAGS", format!("{existing} {ANALYSIS_RUSTFLAGS}"))
        // Takes precedence over RUSTFLAGS when set, which would drop the flags above.
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()?;
    Ok(BuildOutput {
        diagnostics: parse_messages(&String::from_utf8_lossy(&output.stdout)),
        success: output.status.success(),
    })
}

/// Parses cargo's JSON stream (one object per line), keeping only `compiler-message` entries.
pub(crate) fn parse_messages(stdout: &str) -> Vec<Diagnostic> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<CargoMessage>(line).ok())
        .filter(|m| m.reason == "compiler-message")
        .filter_map(|m| m.message)
        .map(|raw| Diagnostic {
            code: raw.code.map(|c| c.code),
            level: raw.level,
            message: raw.message,
            notes: raw.children.into_iter().map(|c| c.message).collect(),
            spans: raw.spans.into_iter().map(RawSpan::into_span).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"reason":"compiler-message","message":{"code":{"code":"unused_must_use","explanation":null},"level":"warning","message":"unused return value of `f` that must be used","spans":[{"file_name":"src\\lib.rs","line_start":7,"column_start":5,"is_primary":true}],"children":[{"message":"CR:3"},{"message":"use `let _ = ...` to ignore the resulting value"}]}}"#;

    #[test]
    fn a_compiler_message_keeps_code_span_column_and_child_notes() {
        let parsed = parse_messages(LINE);
        assert_eq!(parsed.len(), 1);
        let d = &parsed[0];
        assert_eq!(d.code.as_deref(), Some("unused_must_use"));
        assert_eq!(d.notes[0], "CR:3");
        let span = d.primary_span().unwrap();
        assert_eq!(
            (span.file.as_str(), span.line, span.column),
            ("src/lib.rs", 7, 5)
        );
    }

    #[test]
    fn a_span_inside_a_macro_expansion_resolves_to_the_invocation_site() {
        let line = r#"{"reason":"compiler-message","message":{"code":{"code":"deprecated","explanation":null},"level":"warning","message":"use of deprecated function `f`: CR:1","spans":[{"file_name":"src/main.rs","line_start":3,"column_start":29,"is_primary":true,"expansion":{"span":{"file_name":"src/main.rs","line_start":4,"column_start":22,"is_primary":false,"expansion":null},"macro_decl_name":"call!"}}],"children":[]}}"#;
        let span = parse_messages(line)[0].primary_span().unwrap().clone();
        assert_eq!((span.line, span.column), (4, 22));
        assert!(span.is_primary);
    }

    #[test]
    fn non_message_lines_and_garbage_are_skipped() {
        let stdout = format!("{{\"reason\":\"compiler-artifact\"}}\nnot json\n{LINE}\n");
        assert_eq!(parse_messages(&stdout).len(), 1);
    }

    fn diag(level: &str, code: Option<&str>) -> Diagnostic {
        Diagnostic {
            code: code.map(str::to_string),
            level: level.into(),
            message: String::new(),
            notes: vec![],
            spans: vec![],
        }
    }

    #[test]
    fn a_denied_lint_is_not_a_hard_error_but_a_real_error_code_is() {
        assert!(!diag("error", Some("deprecated")).is_hard_error());
        assert!(!diag("warning", None).is_hard_error());
        assert!(diag("error", Some("E0603")).is_hard_error());
        assert!(diag("error", None).is_hard_error());
    }

    #[test]
    fn a_failed_build_with_no_diagnostics_is_broken() {
        let out = BuildOutput {
            diagnostics: vec![],
            success: false,
        };
        assert!(out.is_broken());
        let ok = BuildOutput {
            diagnostics: vec![],
            success: true,
        };
        assert!(!ok.is_broken());
    }
}
