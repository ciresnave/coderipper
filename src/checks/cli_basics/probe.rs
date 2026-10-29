// SPDX-License-Identifier: MIT OR Apache-2.0
//! Runs one binary through the five standard probes, each in a fresh sandbox, and says what it did wrong.
//!
//! A sandbox is a new temporary directory holding the working directory and every directory the environment is pointed at
//! (`HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, the `XDG_*` roots, the temp directory). The environment is cleared first,
//! so the binary sees only `PATH` and the Windows system variables it cannot start without. After the process ends (or is
//! killed) the whole sandbox is listed again and compared with the listing taken before: a file or directory that appeared, a
//! file whose size changed, or one that vanished, is a violation.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a binary may take to answer one probe before it is taken to have started something.
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

/// The most output of each stream that is read; a binary that prints more than this is not answering a version probe.
const OUTPUT_LIMIT: u64 = 1 << 20;

/// What a probe expects.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Exit 0 and a stdout that contains the crate version.
    Version,
    /// Exit 0 and a non-empty stdout.
    Help,
    /// Exit 2 and no panic.
    Refusal,
}

/// How many probes there are.
pub(super) const PROBE_COUNT: usize = PROBES.len();

/// The probes, in the order they run: the arguments and what each must produce.
const PROBES: [(&[&str], Expect); 5] = [
    (&["--version"], Expect::Version),
    (&["-V"], Expect::Version),
    (&["--help"], Expect::Help),
    (&["-h"], Expect::Help),
    (&[UNKNOWN_FLAG], Expect::Refusal),
];

/// The unknown flag the bad-flag probe passes.
pub(super) const UNKNOWN_FLAG: &str = "--coderipper-no-such-flag";

/// One probe that did not behave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Violation {
    /// The arguments, as typed (`--version`).
    pub args: String,
    /// What was wrong, in one line.
    pub problem: String,
    /// The probe ran out of time: the binary started something that does not end.
    pub hung: bool,
}

/// How a process ended.
enum Ended {
    Exited {
        code: Option<i32>,
        out: String,
        err: String,
    },
    TimedOut,
    CouldNotStart(String),
}

/// The five probes of `exe`, each reported when it misbehaves. `version` is the manifest version the version probes must print.
pub(super) fn probe_all(exe: &Path, version: &str, timeout: Duration) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (args, expect) in PROBES {
        if let Some(v) = probe_confirmed(exe, args, expect, version, timeout) {
            violations.push(v);
        }
    }
    violations
}

/// One probe; a timeout is run once more before it is believed (a loaded machine is slow, a daemon is not).
fn probe_confirmed(
    exe: &Path,
    args: &[&str],
    expect: Expect,
    version: &str,
    timeout: Duration,
) -> Option<Violation> {
    let first = probe_once(exe, args, expect, version, timeout);
    match first {
        Some(v) if v.hung => probe_once(exe, args, expect, version, timeout),
        other => other,
    }
}

fn probe_once(
    exe: &Path,
    args: &[&str],
    expect: Expect,
    version: &str,
    timeout: Duration,
) -> Option<Violation> {
    let shown = args.join(" ");
    let violation = |problem: String, hung: bool| Violation {
        args: shown.clone(),
        problem,
        hung,
    };
    let sandbox = match Sandbox::new() {
        Ok(s) => s,
        Err(e) => {
            return Some(violation(
                format!("the probe could not make its sandbox: {e}"),
                false,
            ))
        }
    };
    let before = sandbox.listing();
    let ended = run(exe, args, &sandbox, timeout);
    let after = sandbox.listing();
    let mut problems: Vec<String> = Vec::new();
    match ended {
        Ended::CouldNotStart(e) => problems.push(format!("could not be started: {e}")),
        Ended::TimedOut => {
            let changed = changes(&before, &after);
            let tail = if changed.is_empty() {
                String::new()
            } else {
                format!("; {changed}")
            };
            return Some(violation(
                format!(
                    "no answer within {} s (it started something that does not end){tail}",
                    timeout.as_secs()
                ),
                true,
            ));
        }
        Ended::Exited { code, out, err } => {
            let want = if expect == Expect::Refusal { 2 } else { 0 };
            match code {
                Some(c) if c == want => {}
                Some(c) => problems.push(format!("exit {c}, want {want}")),
                None => problems.push("ended by a signal, not an exit".to_string()),
            }
            if expect != Expect::Refusal && out.trim().is_empty() {
                problems.push("nothing on stdout".to_string());
            }
            if expect == Expect::Version && !out.is_empty() && !out.contains(version) {
                problems.push(format!("stdout does not contain version {version}"));
            }
            if expect == Expect::Refusal {
                // bad input: the message goes to stderr and stdout stays empty
                if err.trim().is_empty() {
                    problems.push("no message on stderr".to_string());
                }
                if !out.trim().is_empty() {
                    problems.push("wrote to stdout on bad input".to_string());
                }
            }
            // output is a pipe here, not a terminal: colour must be off
            if out.contains("\u{1b}[") || err.contains("\u{1b}[") {
                problems
                    .push("ANSI escape sequences in its output though it is a pipe".to_string());
            }
            if looks_like_panic(&out) || looks_like_panic(&err) {
                problems.push("panicked".to_string());
            }
            if problems
                .iter()
                .any(|p| p.starts_with("exit ") || p == "panicked")
            {
                let heard = first_line(if err.trim().is_empty() { &out } else { &err });
                if !heard.is_empty() {
                    problems.push(format!("it said: {heard}"));
                }
            }
        }
    }
    let changed = changes(&before, &after);
    if !changed.is_empty() {
        problems.push(changed);
    }
    (!problems.is_empty()).then(|| violation(problems.join("; "), false))
}

fn looks_like_panic(text: &str) -> bool {
    text.contains("panicked at")
        || text.contains("stack backtrace")
        || text.contains("RUST_BACKTRACE")
}

/// The first non-empty line of `text`, safe to print: control characters and bidi controls become spaces, secrets are scrubbed,
/// and it is cut at 120 characters.
fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let flat: String = line
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let flat = crate::redact::scrub(flat.trim());
    if flat.chars().count() > 120 {
        format!("{}...", flat.chars().take(120).collect::<String>())
    } else {
        flat
    }
}

/// The fresh directories one probe runs in.
struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    const DIRS: [&'static str; 9] = [
        "cwd",
        "home",
        "appdata",
        "localappdata",
        "xdg/config",
        "xdg/data",
        "xdg/cache",
        "xdg/state",
        "tmp",
    ];

    fn new() -> std::io::Result<Self> {
        let root = tempfile::tempdir()?;
        for dir in Self::DIRS {
            std::fs::create_dir_all(root.path().join(dir))?;
        }
        Ok(Self { root })
    }

    fn dir(&self, name: &str) -> std::path::PathBuf {
        self.root.path().join(name)
    }

    /// Every entry under the sandbox: its path relative to the sandbox, whether it is a directory, and its size.
    fn listing(&self) -> BTreeMap<String, (bool, u64)> {
        let mut found = BTreeMap::new();
        let mut dirs = vec![self.root.path().to_path_buf()];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(meta) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                let rel = path
                    .strip_prefix(self.root.path())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if meta.is_dir() {
                    dirs.push(path);
                }
                found.insert(
                    rel,
                    (meta.is_dir(), if meta.is_dir() { 0 } else { meta.len() }),
                );
            }
        }
        found
    }
}

/// What differs between two listings, in one line; empty when they are the same.
fn changes(
    before: &BTreeMap<String, (bool, u64)>,
    after: &BTreeMap<String, (bool, u64)>,
) -> String {
    let mut said: Vec<String> = Vec::new();
    for (path, entry) in after {
        match before.get(path) {
            None => said.push(format!(
                "created {}{}",
                if entry.0 { "directory " } else { "file " },
                shown_path(path)
            )),
            Some(old) if old != entry => said.push(format!("changed {}", shown_path(path))),
            Some(_) => {}
        }
    }
    for path in before.keys().filter(|p| !after.contains_key(*p)) {
        said.push(format!("removed {}", shown_path(path)));
    }
    if said.is_empty() {
        return String::new();
    }
    let total = said.len();
    said.truncate(4);
    let more = if total > 4 {
        format!(" (and {} more)", total - 4)
    } else {
        String::new()
    };
    format!("it touched its surroundings: {}{more}", said.join(", "))
}

/// A sandbox-relative path, safe to print (the name was chosen by the binary).
fn shown_path(path: &str) -> String {
    first_line(path)
}

fn run(exe: &Path, args: &[&str], sandbox: &Sandbox, timeout: Duration) -> Ended {
    let mut command = Command::new(exe);
    command
        .args(args)
        .env_clear()
        .current_dir(sandbox.dir("cwd"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // What the process cannot start without; nothing else is passed on.
    for keep in ["PATH", "SystemRoot", "SYSTEMROOT", "windir", "PATHEXT"] {
        if let Some(value) = std::env::var_os(keep) {
            command.env(keep, value);
        }
    }
    let home = sandbox.dir("home");
    command
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", sandbox.dir("appdata"))
        .env("LOCALAPPDATA", sandbox.dir("localappdata"))
        .env("XDG_CONFIG_HOME", sandbox.dir("xdg/config"))
        .env("XDG_DATA_HOME", sandbox.dir("xdg/data"))
        .env("XDG_CACHE_HOME", sandbox.dir("xdg/cache"))
        .env("XDG_STATE_HOME", sandbox.dir("xdg/state"))
        .env("TMPDIR", sandbox.dir("tmp"))
        .env("TEMP", sandbox.dir("tmp"))
        .env("TMP", sandbox.dir("tmp"));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return Ended::CouldNotStart(first_line(&e.to_string())),
    };
    let read = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(stream) = stream {
                let _ = stream.take(OUTPUT_LIMIT).read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let out = read(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let err = read(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(e) => return Ended::CouldNotStart(first_line(&e.to_string())),
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        // The reader threads end when the pipes close; a grandchild that holds them open is not waited for.
        return Ended::TimedOut;
    };
    Ended::Exited {
        code: status.code(),
        out: out.join().unwrap_or_default(),
        err: err.join().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One small program whose behaviour `mode` picks. `clean` is the control: it answers all five probes and touches nothing.
    const PROGRAM: &str = r#"
fn main() {
    let mode = "MODE";
    let arg = std::env::args().nth(1).unwrap_or_default();
    if mode == "writes-file" {
        std::fs::write("state.db", b"x").unwrap();
    }
    if mode == "writes-home" {
        let home = std::env::var("HOME").unwrap();
        std::fs::create_dir_all(format!("{home}/.fixture")).unwrap();
    }
    match arg.as_str() {
        "--version" | "-V" => {
            if mode == "hangs" {
                std::thread::sleep(std::time::Duration::from_secs(60));
            }
            if mode == "store-fails" {
                eprintln!("error: cannot open store /var/lib/fixture");
                std::process::exit(1);
            }
            if mode == "colours" {
                println!("{}[1mfixture 0.1.0{}[0m", '\u{1b}', '\u{1b}');
            } else if mode == "wrong-version" {
                println!("fixture 9.9.9");
            } else {
                println!("fixture 0.1.0");
            }
        }
        "--help" | "-h" => {
            if mode != "silent-help" {
                println!("usage: fixture [--version]");
            }
        }
        _ => {
            if mode == "panics-on-bad-flag" {
                None::<u8>.unwrap();
            }
            if mode == "accepts-bad-flag" {
                return;
            }
            if mode == "error-on-stdout" {
                println!("error: unknown flag");
            } else {
                eprintln!("error: unknown flag");
            }
            std::process::exit(2);
        }
    }
}
"#;

    fn build(mode: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("main.rs");
        std::fs::write(&src, PROGRAM.replace("MODE", mode)).unwrap();
        let exe = dir.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let out = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .expect("rustc runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (dir, exe)
    }

    fn violations(mode: &str) -> Vec<Violation> {
        let (_dir, exe) = build(mode);
        probe_all(&exe, "0.1.0", Duration::from_secs(1))
    }

    fn flags(found: &[Violation]) -> Vec<&str> {
        found.iter().map(|v| v.args.as_str()).collect()
    }

    #[test]
    fn a_binary_that_answers_the_five_probes_and_touches_nothing_is_clean() {
        assert_eq!(violations("clean"), vec![]);
    }

    #[test]
    fn a_binary_that_writes_a_file_in_its_directory_fails_every_probe() {
        let found = violations("writes-file");
        assert_eq!(found.len(), PROBE_COUNT, "{found:?}");
        assert!(
            found
                .iter()
                .all(|v| v.problem.contains("created file cwd/state.db")),
            "{found:?}"
        );
    }

    #[test]
    fn a_binary_that_writes_under_home_is_caught_in_the_home_sandbox() {
        let found = violations("writes-home");
        assert_eq!(found.len(), PROBE_COUNT, "{found:?}");
        assert!(
            found
                .iter()
                .all(|v| v.problem.contains("created directory home/.fixture")),
            "{found:?}"
        );
    }

    #[test]
    fn a_binary_that_fails_on_a_store_path_instead_of_printing_a_version_is_named_with_what_it_said(
    ) {
        let found = violations("store-fails");
        assert_eq!(flags(&found), vec!["--version", "-V"]);
        assert!(found[0].problem.contains("exit 1, want 0"), "{found:?}");
        assert!(found[0].problem.contains("cannot open store"), "{found:?}");
        assert!(found[0].problem.contains("nothing on stdout"), "{found:?}");
        assert!(!found[0].hung);
    }

    #[test]
    fn a_binary_that_panics_on_an_unknown_flag_fails_only_that_probe() {
        let found = violations("panics-on-bad-flag");
        assert_eq!(flags(&found), vec!["--coderipper-no-such-flag"]);
        assert!(found[0].problem.contains("exit 101, want 2"), "{found:?}");
        assert!(found[0].problem.contains("panicked"), "{found:?}");
    }

    #[test]
    fn a_binary_that_accepts_an_unknown_flag_is_told_it_should_exit_2() {
        let found = violations("accepts-bad-flag");
        assert_eq!(flags(&found), vec!["--coderipper-no-such-flag"]);
        assert!(found[0].problem.contains("exit 0, want 2"), "{found:?}");
    }

    #[test]
    fn a_binary_that_hangs_is_reported_as_hung_after_the_retry() {
        let found = violations("hangs");
        assert_eq!(flags(&found), vec!["--version", "-V"]);
        assert!(
            found
                .iter()
                .all(|v| v.hung && v.problem.contains("no answer within 1 s")),
            "{found:?}"
        );
    }

    #[test]
    fn a_version_line_without_the_manifest_version_fails_both_version_probes() {
        let found = violations("wrong-version");
        assert_eq!(flags(&found), vec!["--version", "-V"]);
        assert!(
            found[0].problem.contains("does not contain version 0.1.0"),
            "{found:?}"
        );
    }

    #[test]
    fn colour_codes_on_a_pipe_fail_the_version_probes() {
        let found = violations("colours");
        assert_eq!(flags(&found), vec!["--version", "-V"]);
        assert!(
            found[0].problem.contains("ANSI escape sequences"),
            "{found:?}"
        );
    }

    #[test]
    fn an_error_message_on_stdout_instead_of_stderr_fails_the_bad_flag_probe() {
        let found = violations("error-on-stdout");
        assert_eq!(flags(&found), vec!["--coderipper-no-such-flag"]);
        assert!(
            found[0].problem.contains("no message on stderr"),
            "{found:?}"
        );
        assert!(
            found[0].problem.contains("wrote to stdout on bad input"),
            "{found:?}"
        );
    }

    #[test]
    fn a_help_that_prints_nothing_fails_both_help_probes() {
        let found = violations("silent-help");
        assert_eq!(flags(&found), vec!["--help", "-h"]);
        assert!(found[0].problem.contains("nothing on stdout"), "{found:?}");
    }

    #[test]
    fn a_program_that_cannot_be_started_is_a_violation_not_a_pass() {
        let found = probe_all(Path::new("no/such/binary"), "0.1.0", Duration::from_secs(1));
        assert_eq!(found.len(), PROBE_COUNT);
        assert!(
            found[0].problem.contains("could not be started"),
            "{found:?}"
        );
    }

    #[test]
    fn the_sandbox_listing_sees_a_changed_removed_and_created_entry() {
        let mut before = BTreeMap::new();
        before.insert("a".to_string(), (false, 1));
        before.insert("b".to_string(), (false, 1));
        let mut after = BTreeMap::new();
        after.insert("a".to_string(), (false, 2));
        after.insert("c".to_string(), (true, 0));
        let said = changes(&before, &after);
        assert!(
            said.contains("changed a")
                && said.contains("removed b")
                && said.contains("created directory c"),
            "{said}"
        );
        assert_eq!(changes(&before, &before), "");
    }
}
