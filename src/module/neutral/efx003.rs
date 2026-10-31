// SPDX-License-Identifier: MIT OR Apache-2.0
//! EFX-003: environment variables are read in one configuration module, not scattered through the code.
//!
//! **What this judges:** every tracked source file (by extension, outside the directories `conflict-marker` skips, outside development
//! tooling and outside test code) is read for a **named** read of the process environment (`std::env::var("X")`, `os.environ["X"]`,
//! `os.getenv("X")`, `process.env.X`, `os.Getenv("X")`, `System.getenv("X")`, `ENV["X"]`, `Environment.GetEnvironmentVariable("X")`,
//! `getenv("X")`). The rule asks for one configuration module; a repository whose reads are in **three or more files** gets one
//! **Low** finding, at the first read, naming the files. The threshold of three is **chosen, not measured**, and the portfolio run
//! showed it does not separate real scatter from noise (the noise was removed by the exclusions below, not by the threshold).
//!
//! **Not a read of configuration, so not counted:**
//! - *forwarding the whole environment*: `env::vars()`, `os.environ.copy()`, `dict(os.environ, ..)`, `os.environ.items()`, `process.env`
//!   passed on as a value (a sandbox, a child process, a test harness);
//! - *platform plumbing*: a read of `PATH`, `HOME`, `USER`, `USERNAME`, `USERPROFILE`, `LOCALAPPDATA`, `APPDATA`, `ProgramFiles`, `TEMP`,
//!   `TMP`, `TMPDIR`, `XDG_*`, `CI`, `CARGO_MANIFEST_DIR`, `OUT_DIR` and a few like them (where the machine keeps things, not a setting);
//! - *text that mentions a read*: a read inside a string literal or a Python docstring (a pattern table, a message, documentation);
//! - *development tooling*: a path under a `tools`, `scripts`, `script`, `probe`, `probes`, `spike`, `spikes`, `xtask`, `docs`, `e2e`,
//!   `bench`, `benches`, `examples` or `.github` directory, a `*.config.js/ts/mjs/cjs` file, and `build.rs`;
//! - *test code*: a `tests`, `test`, `__tests__` or `spec` directory, `tests.rs`, `tests_*.rs`, `*_tests.rs`, `*_test.go`, `test_*.py`,
//!   `*_test.py`, `*.test.*`, `*.spec.*`, and (Rust) everything from the first `#[cfg(test)]` line on.
//!
//! Comment lines are not read. `env!` and `option_env!` (compile time) are not reads. A read whose name is not a literal (`var(name)`)
//! counts, because it cannot be told from a setting.
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

/// Variables that say where the machine keeps things rather than how the program is set up.
const PLATFORM: &[&str] = &[
    "PATH",
    "PATHEXT",
    "HOME",
    "USER",
    "USERNAME",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "PROGRAMDATA",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "PROGRAMFILES",
    "SystemRoot",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "PWD",
    "SHELL",
    "TERM",
    "LANG",
    "CI",
    "CARGO_MANIFEST_DIR",
    "OUT_DIR",
    "PYTHONPATH",
    "PYTHONIOENCODING",
    "LD_LIBRARY_PATH",
];

/// Directories of development tooling and of tests.
const DEV_DIRS: &[&str] = &[
    "tools",
    "scripts",
    "script",
    "probe",
    "probes",
    "spike",
    "spikes",
    "xtask",
    "docs",
    "e2e",
    "bench",
    "benches",
    "examples",
    ".github",
    "tests",
    "test",
    "__tests__",
    "spec",
];

/// The text that opens a named read, per language; the name follows it.
fn reads_for(path: &str) -> Option<&'static [&'static str]> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => &["env::var(", "env::var_os("],
        "py" => &["os.environ[", "os.environ.get(", "os.getenv("],
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => &["process.env.", "process.env["],
        "go" => &["os.Getenv(", "os.LookupEnv("],
        "java" | "kt" | "scala" => &["System.getenv("],
        "rb" => &["ENV[", "ENV.fetch("],
        "cs" => &["Environment.GetEnvironmentVariable("],
        "php" => &["getenv(", "$_ENV["],
        _ => return None,
    })
}

pub(super) fn is_dev_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let (name, dirs) = segments.split_last().unwrap_or((&"", &[]));
    dirs.iter().any(|s| DEV_DIRS.contains(s))
        || *name == "build.rs"
        || *name == "tests.rs"
        || name.starts_with("tests_")
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
        || [".js", ".ts", ".mjs", ".cjs"]
            .iter()
            .any(|e| name.ends_with(&format!(".config{e}")))
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

/// Whether byte `at` of `line` lies inside a `"` or backtick string that opened earlier on the line.
fn in_string(line: &str, at: usize) -> bool {
    let bytes = line.as_bytes();
    let (mut quotes, mut ticks) = (0, 0);
    let mut i = 0;
    while i < at.min(bytes.len()) {
        match bytes[i] {
            b'\\' => i += 1,
            b'"' => quotes += 1,
            b'`' => ticks += 1,
            _ => {}
        }
        i += 1;
    }
    quotes % 2 == 1 || ticks % 2 == 1
}

/// The variable named right after a read's opening text: a quoted literal or a bare identifier (`process.env.NAME`); `None` when
/// the argument is not a literal.
fn name_after(rest: &str) -> Option<&str> {
    let first = rest.chars().next()?;
    if first == '"' || first == '\'' {
        let inner = &rest[1..];
        return inner.find(first).map(|end| &inner[..end]);
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// The 1-based line of the first named read of a non-platform variable in `text`, if the file is a source file that can have one.
fn first_read(path: &str, text: &str) -> Option<u32> {
    let reads = reads_for(path)?;
    let is_rust = path.ends_with(".rs");
    let is_python = path.ends_with(".py");
    let mut docstring: Option<&str> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if is_python {
            // a triple-quoted string opens or closes on each of its delimiters; a read inside one is prose
            let mut open = docstring;
            let mut rest = line;
            while let Some((at, delim)) = ["\"\"\"", "'''"]
                .iter()
                .filter_map(|d| rest.find(d).map(|at| (at, *d)))
                .min_by_key(|(at, _)| *at)
            {
                open = match open {
                    Some(d) if d == delim => None,
                    Some(d) => Some(d),
                    None => Some(delim),
                };
                rest = &rest[at + 3..];
            }
            let inside = docstring.is_some() || open.is_some();
            docstring = open;
            if inside {
                continue;
            }
        }
        if is_rust && trimmed.starts_with("#[cfg(test)]") {
            return None;
        }
        if is_comment(trimmed) {
            continue;
        }
        for read in reads {
            let mut from = 0;
            while let Some(found) = line[from..].find(read) {
                let at = from + found;
                from = at + read.len();
                if in_string(line, at) {
                    continue;
                }
                let name = name_after(&line[from..]);
                if name.is_some_and(|n| PLATFORM.contains(&n) || n.starts_with("XDG_")) {
                    continue;
                }
                return Some(u32::try_from(i + 1).unwrap_or(u32::MAX));
            }
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
        if skipped(path) || is_dev_path(path) || reads_for(path).is_none() {
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
            ("e.test.ts", "process.env.A\n"),
            ("src/tests.rs", RS),
            ("src/tests_x.rs", RS),
            (".github/gate.py", "os.environ['A']\n"),
        ]);
        assert_eq!(f.len(), 1);
        assert!(
            f[0].summary.contains("a.py, b.ts, c.go"),
            "{}",
            f[0].summary
        );
    }

    /// Each class of false positive the portfolio run found, one file apiece, plus three real readers: only the three are counted.
    #[test]
    fn forwarding_platform_plumbing_prose_and_tooling_are_not_reads_of_configuration() {
        let forward_rs = "fn f() { let e: Vec<_> = std::env::vars().collect(); }\n";
        let forward_py = "import os\nenv = dict(os.environ, A='1')\nc = os.environ.copy()\n";
        let forward_js = "run(process.argv, process.env);\n";
        let platform = "fn f() { let _ = std::env::var_os(\"PATH\"); let _ = std::env::var(\"USERNAME\"); let _ = std::env::var_os(\"XDG_STATE_HOME\"); }\n";
        let table = "const T: &[(&str, u8)] = &[(\"env::var_os(\", 5)];\n";
        let doc = "def f():\n    \"\"\"os.environ.get('A') would report absent\n    and os.getenv('B') too.\"\"\"\n    return 1\n";
        let tool = "import os\nx = os.environ['A']\n";
        let f = found(&[
            ("a/src/f.rs", forward_rs),
            ("b/src/f.py", forward_py),
            ("c/src/f.js", forward_js),
            ("d/src/p.rs", platform),
            ("e/src/t.rs", table),
            ("f/src/d.py", doc),
            ("tools/t.py", tool),
            ("scripts/s.py", tool),
            ("probe/p.py", tool),
            ("vite.config.ts", "const b = process.env.BACKEND_URL;\n"),
            ("src/r1.rs", RS),
            ("src/r2.rs", RS),
            ("src/r3.rs", RS),
        ]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("3 source files"), "{}", f[0].summary);
    }

    #[test]
    fn a_read_after_a_docstring_closes_still_counts() {
        let src = "\"\"\"doc os.environ['A']\"\"\"\nx = os.environ['B']\n";
        assert_eq!(first_read("a.py", src), Some(2));
    }

    #[test]
    fn a_name_that_is_not_a_literal_counts() {
        assert_eq!(
            first_read("a.rs", "fn f(n: &str) { let _ = std::env::var(n); }\n"),
            Some(1)
        );
    }
}
