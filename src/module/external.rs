//! [`ExternalModule`]: a module that is a separate executable, spoken to over JSON lines.

use std::ffi::OsString;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{ErrorKind, Event, Hello, Limits, Module, ModuleFailure, ModuleOutput, Request};

/// The environment a module is started with; everything else is scrubbed (design §5.7). Enough to find its tools and a
/// temporary directory, nothing that carries a credential.
pub(super) const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "HOME",
    "USERPROFILE",
    "LANG",
    "LC_ALL",
];

/// How long `describe` may take.
const DESCRIBE_WALL: Duration = Duration::from_secs(30);

/// How much of a module's stderr is kept for an error message.
const STDERR_TAIL_BYTES: usize = 4096;

/// A module run as a child process: `<program> [args...] describe` and `<program> [args...] check`.
///
/// Every failure is an outcome, never a panic and never silence: a module that crashes, hangs, prints too much, prints
/// garbage or prints no summary is judged by [`super::reconcile`] into rules that gave no verdict. On a timeout or a limit
/// the whole process tree is killed (`taskkill /T` on Windows, a process group elsewhere; a descendant that leaves its
/// group on Unix, with `setsid`, escapes; a job object or cgroup would close that and is not used yet).
///
/// The host does not choose which executable to run from the analysed project: the caller of this type does (design §5.2).
/// The child gets a scrubbed environment and an empty scratch directory as its working directory.
#[derive(Debug, Clone)]
pub struct ExternalModule {
    program: PathBuf,
    args: Vec<OsString>,
}

impl ExternalModule {
    /// A module run as `program`.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    /// Arguments placed before `describe` / `check` (an interpreter's script, say).
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    fn command(&self, subcommand: &str, cwd: &std::path::Path) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .arg(subcommand)
            .env_clear()
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in ENV_ALLOWLIST {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command
    }
}

impl Module for ExternalModule {
    fn describe(&self) -> anyhow::Result<Hello> {
        let scratch = tempfile::tempdir()?;
        let limits = Limits::new(DESCRIBE_WALL.as_secs(), 1024 * 1024);
        let run = run_child(
            self.command("describe", scratch.path()),
            None,
            &limits,
            Who::Module,
        );
        if let Some(failure) = run.failure {
            anyhow::bail!("{}: {}", failure.kind, failure.detail);
        }
        let text = run.lines.join("\n");
        let hello: Hello = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("the hello is not readable ({e}): {text}"))?;
        if let Some(problem) = hello.problem() {
            anyhow::bail!("{problem}");
        }
        Ok(hello)
    }

    fn check(&self, request: &Request) -> ModuleOutput {
        let scratch = match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => {
                return ModuleOutput::failed(
                    ErrorKind::Internal,
                    format!("cannot make a scratch directory: {e}"),
                )
            }
        };
        let line = match serde_json::to_string(request) {
            Ok(line) => line,
            Err(e) => {
                return ModuleOutput::failed(
                    ErrorKind::Internal,
                    format!("cannot serialise the request: {e}"),
                )
            }
        };
        let run = run_child(
            self.command("check", scratch.path()),
            Some(&line),
            &request.limits,
            Who::Module,
        );
        let mut output = ModuleOutput::default();
        let mut findings = 0;
        for line in run.lines {
            if line.trim().is_empty() {
                continue;
            }
            match Event::parse(&line) {
                Ok(Some(event)) => {
                    if matches!(event, Event::Finding(_)) {
                        findings += 1;
                    }
                    output.events.push(event);
                }
                Ok(None) => {}
                Err(why) => output.unreadable.push(format!("{why}: {}", clip(&line))),
            }
        }
        output.failure = run.failure.or_else(|| {
            (findings > request.limits.max_findings).then(|| {
                ModuleFailure::new(
                    ErrorKind::LimitExceeded,
                    format!(
                        "{findings} findings, past the limit of {}",
                        request.limits.max_findings
                    ),
                )
            })
        });
        output
    }
}

/// A line of module output, cut short for an error message.
fn clip(line: &str) -> String {
    line.chars().take(200).collect()
}

/// What running a child produced.
pub(super) struct ChildRun {
    pub(super) lines: Vec<String>,
    pub(super) failure: Option<ModuleFailure>,
}

/// What a child process is: a module (speaks the protocol) or a delegated tool (a program whose output the host reads). Only
/// the words of a failure and the kind of a crash differ.
#[derive(Clone, Copy)]
pub(super) enum Who {
    Module,
    Tool,
}

impl Who {
    fn noun(self) -> &'static str {
        match self {
            Who::Module => "module",
            Who::Tool => "tool",
        }
    }

    fn crashed(self) -> ErrorKind {
        match self {
            Who::Module => ErrorKind::ModuleCrashed,
            Who::Tool => ErrorKind::ToolFailed,
        }
    }
}

enum Piece {
    Line(String),
    TooLarge,
    Done,
}

/// Runs a prepared command: writes `stdin_line` to its stdin, reads its stdout as lines against `limits`, keeps the tail
/// of its stderr, and kills the process tree on a timeout or a limit.
pub(super) fn run_child(
    mut command: Command,
    stdin_line: Option<&str>,
    limits: &Limits,
    who: Who,
) -> ChildRun {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(limits.wall_secs);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return ChildRun {
                lines: Vec::new(),
                failure: Some(ModuleFailure::new(
                    who.crashed(),
                    format!("cannot start the {}: {e}", who.noun()),
                )),
            }
        }
    };

    // Written from a thread: a module that does not read its stdin must not hold the host past the wall-clock limit (the
    // request can be larger than a pipe buffer). When the process is killed the write fails and the thread ends.
    if let (Some(mut stdin), Some(line)) = (child.stdin.take(), stdin_line.map(str::to_string)) {
        std::thread::spawn(move || {
            // a module that exits before reading is judged by its exit status, not by this write
            let _ = writeln!(stdin, "{line}");
        });
    }

    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        let cap = limits.max_output_bytes;
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stdout);
            let mut total = 0usize;
            loop {
                let mut buffer = Vec::new();
                // `take` bounds one line, so a module printing without a newline cannot grow this without limit
                let room = (cap.saturating_sub(total) as u64).saturating_add(1);
                match (&mut reader).take(room).read_until(b'\n', &mut buffer) {
                    Ok(0) | Err(_) => {
                        let _ = sender.send(Piece::Done);
                        return;
                    }
                    Ok(n) => {
                        total += n;
                        if total > cap {
                            let _ = sender.send(Piece::TooLarge);
                            return;
                        }
                        let text = String::from_utf8_lossy(&buffer)
                            .trim_end_matches(['\n', '\r'])
                            .to_string();
                        if sender.send(Piece::Line(text)).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }
    let stderr_tail = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    if let Some(mut stderr) = child.stderr.take() {
        let tail = std::sync::Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 1024];
            while let Ok(n) = stderr.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if let Ok(mut tail) = tail.lock() {
                    tail.extend_from_slice(&chunk[..n]);
                    let excess = tail.len().saturating_sub(STDERR_TAIL_BYTES);
                    tail.drain(..excess);
                }
            }
        });
    }
    let stderr_text = || {
        // the reader may still be flushing: give it a moment, the tail is only for a message
        std::thread::sleep(Duration::from_millis(50));
        stderr_tail
            .lock()
            .map(|t| String::from_utf8_lossy(&t).trim().to_string())
            .unwrap_or_default()
    };
    let with_stderr = |what: String, text: String| {
        if text.is_empty() {
            what
        } else {
            format!("{what}; stderr: {text}")
        }
    };

    let mut lines = Vec::new();
    let mut failure = None;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(wait) {
            Ok(Piece::Line(line)) => lines.push(line),
            Ok(Piece::Done) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(Piece::TooLarge) => {
                kill_tree(&mut child);
                failure = Some(ModuleFailure::new(
                    ErrorKind::LimitExceeded,
                    format!(
                        "more than {} bytes of output (the limit)",
                        limits.max_output_bytes
                    ),
                ));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                kill_tree(&mut child);
                failure = Some(ModuleFailure::new(
                    ErrorKind::Timeout,
                    format!("past the {}s wall-clock limit", limits.wall_secs),
                ));
                break;
            }
        }
    }
    if failure.is_some() {
        return ChildRun { lines, failure };
    }

    // stdout is closed: the module is exiting, or has closed its output and carried on
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    failure = Some(ModuleFailure::new(
                        who.crashed(),
                        with_stderr(
                            format!("the {} exited with {status}", who.noun()),
                            stderr_text(),
                        ),
                    ));
                }
                break;
            }
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                failure = Some(ModuleFailure::new(
                    ErrorKind::Timeout,
                    format!(
                        "past the {}s wall-clock limit (output closed, process still running)",
                        limits.wall_secs
                    ),
                ));
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                failure = Some(ModuleFailure::new(
                    ErrorKind::Internal,
                    format!("cannot wait for the {}: {e}", who.noun()),
                ));
                break;
            }
        }
    }
    ChildRun { lines, failure }
}

/// Kills the child and everything it started, then reaps it.
pub(super) fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // the child leads its own process group (see `command`), so the group id is its pid
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
