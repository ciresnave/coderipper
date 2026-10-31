// SPDX-License-Identifier: MIT OR Apache-2.0
//! EFX-003: environment variables are read in one configuration module, not scattered through the code.
//!
//! **What this judges:** every tracked source file (by extension, outside the directories `conflict-marker` skips and outside test
//! code) is read for a call that reads the process environment (`std::env::var`, `os.environ`, `os.getenv`, `process.env`,
//! `os.Getenv`, `System.getenv`, `ENV[..]`, `Environment.GetEnvironmentVariable`, `getenv`). The rule asks for one configuration
//! module; a repository whose reads are in **three or more files** gets one **Low** finding, at the first read, naming the files. The
//! threshold of three is **chosen, not measured**: one configuration module, one entry point that picks a profile, and one more
//! that a reader can justify are allowed; a fourth read site is a sign of scatter. A repository with two files that read is not flagged.
//!
//! **Test code is not counted:** a path with a `tests`, `test`, `__tests__`, `spec`, `benches`, `examples` or `.github` directory (CI gate scripts), `build.rs`, a
//! `tests.rs`, `tests_*.rs`, `*_tests.rs`, `*_test.go`, `test_*.py`, `*_test.py`, `*.test.*` or `*.spec.*` file, and (Rust) everything from the first `#[cfg(test)]` line on.
//! Comment lines are not read. `env!` and `option_env!` (compile time) are not reads.
//!
//! **What it cannot see:** a read through a wrapper or a re-export under another name, shell scripts (the environment is the
//! language there), a read in a language not listed, and whether the one module that reads is itself a configuration module.

use super::conflict_marker::SKIPPED;
use super::{RepoView, Verdict};
use crate::finding::{Confidence, Finding, Location, Severity};

const ID: &str = crate::rule_id::EFX_003;

/// Files with a read at which the rule fires. Chosen, not measured.
const THRESHOLD: usize = 3;

const MAX_BYTES: usize = 2 * 1024 * 1024;

/// The prefilter handed to `git grep -E`: any text that could be one of the reads below.
const PREFILTER: &str = "(env::var|environ|getenv|process\\.env|Getenv|LookupEnv|ENV\\[|ENV\\.fetch|GetEnvironmentVariable)";

fn reads_for(path: &str) -> Option<&'static [&'static str]> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => &["env::var(", "env::var_os(", "env::vars(", "env::vars_os("],
        "py" => &["os.environ", "os.getenv("],
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => &["process.env"],
        "go" => &["os.Getenv(", "os.LookupEnv(", "os.Environ("],
        "java" | "kt" | "scala" => &["System.getenv("],
        "rb" => &["ENV[", "ENV.fetch("],
        "cs" => &["Environment.GetEnvironmentVariable("],
        "php" => &["getenv(", "$_ENV["],
        _ => return None,
    })
}

fn is_test_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let (name, dirs) = segments.split_last().unwrap_or((&"", &[]));
    dirs.iter().any(|s| {
        matches!(
            *s,
            "tests" | "test" | "__tests__" | "spec" | "benches" | "examples" | ".github"
        )
    }) || *name == "build.rs"
        || *name == "tests.rs"
        || name.starts_with("tests_")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
}

fn skipped(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    segments[..segments.len() - 1]
        .iter()
        .any(|s| SKIPPED.contains(s))
}

fn is_comment(trimmed: &str) -> bool {
    ["//", "#", "*", "/*", "--"]
        .iter()
        .any(|p| trimmed.starts_with(p))
}

/// The 1-based line of the first environment read in `text`, if the file is a source file that can have one.
fn first_read(path: &str, text: &str) -> Option<u32> {
    let reads = reads_for(path)?;
    let is_rust = path.ends_with(".rs");
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if is_rust && trimmed.starts_with("#[cfg(test)]") {
            return None;
        }
        if is_comment(trimmed) {
            continue;
        }
        if reads.iter().any(|r| line.contains(r)) {
            return Some(u32::try_from(i + 1).unwrap_or(u32::MAX));
        }
    }
    None
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let candidates = view.paths_matching(PREFILTER);
    let everything;
    let paths: Vec<&String> = match &candidates {
        Some(found) => found.iter().collect(),
        None => {
            everything = view.paths();
            everything.iter().collect()
        }
    };
    let mut readers: Vec<(String, u32)> = Vec::new();
    let mut read = 0usize;
    for path in paths {
        if skipped(path) || is_test_path(path) || reads_for(path).is_none() {
            continue;
        }
        let Some(text) = view.read(path) else {
            continue;
        };
        if text.len() > MAX_BYTES || text.contains('\0') {
            continue;
        }
        read += 1;
        if let Some(line) = first_read(path, &text) {
            readers.push((path.clone(), line));
        }
    }
    readers.sort();
    if readers.len() < THRESHOLD {
        return Verdict::Findings(Vec::new());
    }
    let (first, line) = readers[0].clone();
    let names: Vec<&str> = readers.iter().take(5).map(|(p, _)| p.as_str()).collect();
    let more = readers.len().saturating_sub(names.len());
    let list = if more == 0 {
        names.join(", ")
    } else {
        format!("{}, and {more} more", names.join(", "))
    };
    Verdict::Findings(vec![Finding::new(
        ID,
        Severity::Low,
        Confidence::Medium,
        view.name(),
        format!(
            "environment variables are read in {} source files ({list}), not in one configuration module (the rule fires at {THRESHOLD})",
            readers.len()
        ),
        "Read the environment once, in a configuration module, parse it into typed settings and pass those on; or waive the repository with an `[[allow]]` entry that says why.",
    )
    .location(Location::new(first.as_str(), Some(line)))
    .subject(view.name())
    .positive_control(format!(
        "{read} source file(s) that can read the environment were read; {} of them do, the first at {first}:{line}",
        readers.len()
    ))])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(files: &[(&str, &str)]) -> Vec<Finding> {
        let v = RepoView::from_files(files);
        match judge(&v) {
            Verdict::Findings(f) => f,
            _ => panic!("expected findings"),
        }
    }

    const RS: &str = "fn f() { let _ = std::env::var(\"A\"); }\n";

    #[test]
    fn three_files_that_read_the_environment_are_one_finding_and_two_are_not() {
        assert!(found(&[("src/a.rs", RS), ("src/b.rs", RS)]).is_empty());
        let f = found(&[("src/a.rs", RS), ("src/b.rs", RS), ("src/c.rs", RS)]);
        assert_eq!(f.len(), 1);
        assert!(f[0].summary.contains("3 source files"), "{}", f[0].summary);
    }

    #[test]
    fn test_code_comments_and_compile_time_reads_are_not_counted() {
        let in_test =
            "fn f() {}\n#[cfg(test)]\nmod t { fn g() { let _ = std::env::var(\"A\"); } }\n";
        let comment = "// std::env::var(\"A\")\nfn f() {}\n";
        let compile = "const X: &str = env!(\"A\"); const Y: Option<&str> = option_env!(\"B\");\n";
        let f = found(&[
            ("src/a.rs", RS),
            ("src/b.rs", in_test),
            ("src/c.rs", comment),
            ("src/d.rs", compile),
            ("tests/e.rs", RS),
            ("build.rs", RS),
            ("fixtures/f.rs", RS),
            ("src/g.rs", RS),
        ]);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn other_languages_count_and_the_files_are_named() {
        let f = found(&[
            ("a.py", "import os\nx = os.environ['A']\n"),
            ("b.ts", "const x = process.env.A;\n"),
            ("c.go", "package c\nvar x = os.Getenv(\"A\")\n"),
            ("test_d.py", "os.getenv('A')\n"),
            ("src/tests.rs", RS),
            ("src/tests_x.rs", RS),
            (".github/gate.py", "os.environ['A']\n"),
            ("e.test.ts", "process.env.A\n"),
        ]);
        assert_eq!(f.len(), 1);
        assert!(
            f[0].summary.contains("a.py, b.ts, c.go"),
            "{}",
            f[0].summary
        );
    }
}
