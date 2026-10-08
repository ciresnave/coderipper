//! The module protocol against a real child process: every way an external module can fail ends as an error, never as a
//! clean run.
//!
//! This file is its own harness (`harness = false` in Cargo.toml) because the module under test has to be an executable and
//! this test binary is the only one a test can rely on: run as `<this exe> [--mode M] describe|check` it IS the fake module;
//! run any other way it runs the tests below.

use std::path::Path;
use std::process::ExitCode;

use coderipper::check::{CheckContext, Tier};
use coderipper::finding::{Confidence, Finding, Severity};
use coderipper::module::{
    Capabilities, ErrorKind, Event, ExternalModule, Hello, Limits, Module, ModuleSummary, Request,
    RuleClaim, RuleRan, RuleResult,
};
use coderipper::{run_module, RunResult};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.last().map(String::as_str) {
        Some(sub @ ("describe" | "check")) => {
            fake_module(sub, &args[..args.len() - 1]);
            ExitCode::SUCCESS
        }
        _ => run_tests(&args),
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The fake module

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}

fn say(event: Event) {
    println!("{}", event.to_line().unwrap());
}

fn finding(rule: &str) -> Event {
    Event::Finding(Box::new(Finding::new(
        rule,
        Severity::Medium,
        Confidence::High,
        "fake",
        format!("{rule} found something"),
        "detail",
    )))
}

fn summary() -> Event {
    Event::Summary(ModuleSummary::default())
}

fn skip(rule: &str) -> Event {
    let mut result = RuleResult::ran(rule, 0);
    result.status = RuleRan::Skipped;
    result.reason_code = Some("not_applicable_here".into());
    result.detail = Some("no tsconfig.json in this project".into());
    Event::RuleResult(result)
}

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

fn fake_module(sub: &str, args: &[String]) {
    let mode = flag(args, "--mode").unwrap_or("ok");
    if mode == "sleep" || mode == "deaf" {
        // "deaf" never reads its stdin
        sleep_forever();
    }
    if sub == "describe" {
        match mode {
            "hello-garbage" => println!("this is not a hello"),
            "hello-crash" => {
                eprintln!("describe blew up");
                std::process::exit(7);
            }
            "wrong-protocol" => {
                let mut hello = hello();
                hello.protocol = vec!["9.0".into()];
                println!("{}", serde_json::to_string(&hello).unwrap());
            }
            _ => println!("{}", serde_json::to_string(&hello()).unwrap()),
        }
        return;
    }

    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let request: Request = serde_json::from_str(&line).expect("the host sent a readable request");
    let rules = request.rules.clone();
    let first = rules[0].as_str();
    match mode {
        "ok" => {
            say(finding(first));
            for (i, rule) in rules.iter().enumerate() {
                say(Event::RuleResult(RuleResult::ran(
                    rule,
                    usize::from(i == 0),
                )));
            }
            say(summary());
        }
        "crash" => {
            say(finding(first));
            eprintln!("fake module: out of cheese");
            std::process::exit(3);
        }
        "hang" => sleep_forever(),
        "hang-child" => {
            let dir = flag(args, "--dir").expect("--dir");
            // deliberately never waited on: the host must kill it, with the rest of the tree
            #[allow(clippy::zombie_processes)]
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--mode", "sleep", "describe"])
                .spawn()
                .unwrap();
            std::fs::write(Path::new(dir).join("pid"), child.id().to_string()).unwrap();
            sleep_forever()
        }
        "no-summary" => {
            for rule in &rules {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
        }
        "missing-result" => {
            for rule in &rules[..rules.len() - 1] {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "garbage" => {
            println!("this is not json");
            for rule in &rules {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "huge" => {
            for _ in 0..100 {
                println!(
                    "{{\"reason\":\"coderipper-progress\",\"what\":\"{}\"}}",
                    "x".repeat(1000)
                );
            }
            for rule in &rules {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "no-newline" => {
            // one endless line: the host must not buffer it without bound
            let chunk = "y".repeat(4096);
            loop {
                print!("{chunk}");
                use std::io::Write;
                if std::io::stdout().flush().is_err() {
                    return;
                }
            }
        }
        "wrong-count" => {
            say(finding(first));
            say(Event::RuleResult(RuleResult::ran(first, 2)));
            for rule in &rules[1..] {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "duplicate" => {
            for rule in &rules {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(Event::RuleResult(RuleResult::ran(first, 0)));
            say(Event::RuleResult(RuleResult::ran("never-asked-for", 0)));
            say(summary());
        }
        "skip-all" => {
            for rule in &rules {
                say(skip(rule));
            }
            say(summary());
        }
        "skip-one" => {
            say(skip(first));
            for rule in &rules[1..] {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "silent" => {}
        "many-findings" => {
            for _ in 0..3 {
                say(finding(first));
            }
            say(Event::RuleResult(RuleResult::ran(first, 3)));
            for rule in &rules[1..] {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "error-result" => {
            say(Event::RuleResult(RuleResult::error(
                first,
                ErrorKind::ToolMissing,
                "eslint is not installed",
            )));
            for rule in &rules[1..] {
                say(Event::RuleResult(RuleResult::ran(rule, 0)));
            }
            say(summary());
        }
        "env" => {
            // reports whether a variable the host had is visible to the module
            let leaked = std::env::var_os("CODERIPPER_TEST_SECRET").is_some();
            for rule in &rules {
                if leaked {
                    say(finding(rule));
                }
                say(Event::RuleResult(RuleResult::ran(
                    rule,
                    usize::from(leaked),
                )));
            }
            say(summary());
        }
        other => panic!("unknown mode {other}"),
    }
}

fn hello() -> Hello {
    Hello::new(
        "fake",
        "0.0.1",
        vec!["fake".into()],
        vec!["fake.toml".into()],
        Capabilities::default(),
        vec![RuleClaim::native("r1"), RuleClaim::native("r2")],
    )
}

// ---------------------------------------------------------------------------------------------------------------------
// The tests

fn module(mode: &str) -> ExternalModule {
    ExternalModule::new(std::env::current_exe().unwrap()).args(["--mode", mode])
}

fn run(mode: &str, limits: Limits) -> RunResult {
    let dir = tempfile::tempdir().unwrap();
    run_module(
        &module(mode),
        &CheckContext::new(dir.path()),
        Tier::Fast,
        limits,
    )
}

fn run_default(mode: &str) -> RunResult {
    run(mode, Limits::default())
}

fn short(wall_secs: u64) -> Limits {
    Limits::new(wall_secs, 16 * 1024 * 1024)
}

fn assert_error(result: &RunResult, needle: &str) {
    assert!(
        result.errors.iter().any(|e| e.contains(needle)),
        "expected an error mentioning {needle:?}, got {:?}",
        result.errors
    );
}

fn a_well_behaved_module_gives_its_findings_and_no_errors() {
    let result = run_default("ok");
    assert_eq!(result.errors, Vec::<String>::new());
    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    assert_eq!(result.findings[0].check_id, "r1");
}

fn a_module_that_crashes_is_an_error_naming_the_exit_and_stderr() {
    let result = run_default("crash");
    assert_error(&result, "module_crashed");
    assert_error(&result, "out of cheese");
    // the rules never got a result, so none is clean; the finding it printed first is kept, flagged by the error
    assert_error(&result, "r1 failed to run");
    assert_error(&result, "r2 failed to run");
}

fn a_module_that_hangs_is_killed_and_ends_as_a_timeout() {
    let started = std::time::Instant::now();
    let result = run("hang", short(1));
    assert!(started.elapsed().as_secs() < 20, "the host waited too long");
    assert_error(&result, "timeout");
    assert_error(&result, "r1 failed to run");
}

fn a_hang_kills_the_whole_process_tree_not_just_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let module = ExternalModule::new(std::env::current_exe().unwrap()).args([
        "--mode".to_string(),
        "hang-child".to_string(),
        "--dir".to_string(),
        dir.path().to_string_lossy().into_owned(),
    ]);
    let result = run_module(
        &module,
        &CheckContext::new(tempfile::tempdir().unwrap().path()),
        Tier::Fast,
        short(2),
    );
    assert_error(&result, "timeout");
    let pid: u32 = std::fs::read_to_string(dir.path().join("pid"))
        .expect("the fake module started a grandchild")
        .trim()
        .parse()
        .unwrap();
    // give the OS a moment to finish tearing the process down
    let mut alive = true;
    for _ in 0..50 {
        alive = process_alive(pid);
        if !alive {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if alive {
        kill_pid(pid);
    }
    assert!(!alive, "the grandchild {pid} survived the timeout");
}

fn process_alive(pid: u32) -> bool {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    } else {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn kill_pid(pid: u32) {
    if cfg!(windows) {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    } else {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}

fn a_module_that_never_reads_a_large_request_cannot_outlast_the_wall_clock() {
    let rules: Vec<String> = (0..50_000).map(|i| format!("rule-number-{i}")).collect();
    let dir = tempfile::tempdir().unwrap();
    let request = Request::new(
        &CheckContext::new(dir.path()),
        None,
        Tier::Fast,
        rules,
        short(1),
    );
    let started = std::time::Instant::now();
    let output = module("deaf").check(&request);
    assert!(
        started.elapsed().as_secs() < 20,
        "the host hung writing the request"
    );
    assert_eq!(output.failure.map(|f| f.kind), Some(ErrorKind::Timeout));
}

fn a_module_that_prints_no_summary_is_an_incomplete_run() {
    let result = run_default("no-summary");
    assert_error(&result, "without a coderipper-module-summary");
    assert!(result.findings.is_empty());
}

fn a_requested_rule_the_module_never_answers_is_no_verdict() {
    let result = run_default("missing-result");
    assert_error(&result, "r2 failed to run (no_verdict)");
    assert!(
        !result.errors.iter().any(|e| e.starts_with("r1 ")),
        "{:?}",
        result.errors
    );
}

fn a_garbage_line_is_a_protocol_error_but_the_valid_results_stand() {
    let result = run_default("garbage");
    assert_error(&result, "unreadable line");
    assert_error(&result, "this is not json");
    assert!(!result.errors.iter().any(|e| e.contains("failed to run")));
}

fn output_past_the_limit_kills_the_module_and_is_a_limit_error() {
    let result = run("huge", Limits::new(30, 5000));
    assert_error(&result, "limit_exceeded");
    assert_error(&result, "more than 5000 bytes");
}

fn one_endless_line_is_stopped_by_the_output_limit() {
    let started = std::time::Instant::now();
    let result = run("no-newline", Limits::new(30, 100_000));
    assert!(started.elapsed().as_secs() < 20);
    assert_error(&result, "limit_exceeded");
}

fn more_findings_than_the_limit_is_a_limit_error() {
    let mut limits = Limits::default();
    limits.max_findings = 2;
    let result = run("many-findings", limits);
    assert_error(&result, "limit_exceeded");
    assert_error(&result, "3 findings");
}

fn a_wrong_finding_count_makes_the_rule_a_protocol_mismatch() {
    let result = run_default("wrong-count");
    assert_error(&result, "r1 failed to run (protocol_mismatch)");
    assert!(
        result.findings.is_empty(),
        "an untrusted rule's findings are dropped"
    );
}

fn a_duplicate_and_an_unrequested_result_are_recorded() {
    let result = run_default("duplicate");
    assert_error(&result, "a second result for \"r1\"");
    assert_error(&result, "\"never-asked-for\", which was not requested");
}

fn a_module_that_skips_everything_gave_no_verdict() {
    let result = run_default("skip-all");
    assert_error(&result, "r1 failed to run (no_verdict)");
    assert_error(&result, "r2 failed to run (no_verdict)");
}

fn one_not_applicable_skip_is_a_gap_and_not_an_error() {
    let result = run_default("skip-one");
    assert_eq!(result.errors, Vec::<String>::new());
    assert!(result.findings.is_empty());
}

fn a_module_that_says_nothing_is_not_a_clean_run() {
    let result = run_default("silent");
    assert_error(&result, "r1 failed to run (no_verdict)");
    assert_error(&result, "r2 failed to run (no_verdict)");
}

fn an_error_result_keeps_its_kind_and_detail() {
    let result = run_default("error-result");
    assert_error(
        &result,
        "r1 failed to run (tool_missing): eslint is not installed",
    );
}

fn the_module_does_not_inherit_the_hosts_environment() {
    std::env::set_var("CODERIPPER_TEST_SECRET", "hunter2");
    let result = run_default("env");
    std::env::remove_var("CODERIPPER_TEST_SECRET");
    assert_eq!(result.errors, Vec::<String>::new());
    assert!(
        result.findings.is_empty(),
        "the module saw the host's variable: {:?}",
        result.findings
    );
}

fn a_broken_hello_is_an_error_not_a_run() {
    // these modes break only `describe`
    for mode in ["hello-garbage", "hello-crash", "wrong-protocol"] {
        let result = run_default(mode);
        assert_error(&result, "the module's hello");
        assert!(result.findings.is_empty(), "{mode}");
    }
}

fn a_program_that_does_not_exist_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let result = run_module(
        &ExternalModule::new(dir.path().join("coderipper-module-nothing")),
        &CheckContext::new(dir.path()),
        Tier::Fast,
        Limits::default(),
    );
    assert_error(&result, "the module's hello");
}

fn the_rust_module_describes_the_five_checks() {
    let checks = coderipper::registered_checks();
    let hello = coderipper::module::RustModule::new(&checks)
        .describe()
        .unwrap();
    assert_eq!(hello.problem(), None);
    let ids: Vec<&str> = hello.rules.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "reachability",
            "unused-return-values",
            "unused-parameters",
            "version-consistency",
            "ci-protection-presence"
        ]
    );
    assert_eq!(hello.languages, ["rust"]);
}

fn run_tests(args: &[String]) -> ExitCode {
    let tests: &[(&str, fn())] = &[
        (
            "a_well_behaved_module_gives_its_findings_and_no_errors",
            a_well_behaved_module_gives_its_findings_and_no_errors,
        ),
        (
            "a_module_that_crashes_is_an_error_naming_the_exit_and_stderr",
            a_module_that_crashes_is_an_error_naming_the_exit_and_stderr,
        ),
        (
            "a_module_that_hangs_is_killed_and_ends_as_a_timeout",
            a_module_that_hangs_is_killed_and_ends_as_a_timeout,
        ),
        (
            "a_hang_kills_the_whole_process_tree_not_just_the_child",
            a_hang_kills_the_whole_process_tree_not_just_the_child,
        ),
        (
            "a_module_that_never_reads_a_large_request_cannot_outlast_the_wall_clock",
            a_module_that_never_reads_a_large_request_cannot_outlast_the_wall_clock,
        ),
        (
            "a_module_that_prints_no_summary_is_an_incomplete_run",
            a_module_that_prints_no_summary_is_an_incomplete_run,
        ),
        (
            "a_requested_rule_the_module_never_answers_is_no_verdict",
            a_requested_rule_the_module_never_answers_is_no_verdict,
        ),
        (
            "a_garbage_line_is_a_protocol_error_but_the_valid_results_stand",
            a_garbage_line_is_a_protocol_error_but_the_valid_results_stand,
        ),
        (
            "output_past_the_limit_kills_the_module_and_is_a_limit_error",
            output_past_the_limit_kills_the_module_and_is_a_limit_error,
        ),
        (
            "one_endless_line_is_stopped_by_the_output_limit",
            one_endless_line_is_stopped_by_the_output_limit,
        ),
        (
            "more_findings_than_the_limit_is_a_limit_error",
            more_findings_than_the_limit_is_a_limit_error,
        ),
        (
            "a_wrong_finding_count_makes_the_rule_a_protocol_mismatch",
            a_wrong_finding_count_makes_the_rule_a_protocol_mismatch,
        ),
        (
            "a_duplicate_and_an_unrequested_result_are_recorded",
            a_duplicate_and_an_unrequested_result_are_recorded,
        ),
        (
            "a_module_that_skips_everything_gave_no_verdict",
            a_module_that_skips_everything_gave_no_verdict,
        ),
        (
            "one_not_applicable_skip_is_a_gap_and_not_an_error",
            one_not_applicable_skip_is_a_gap_and_not_an_error,
        ),
        (
            "a_module_that_says_nothing_is_not_a_clean_run",
            a_module_that_says_nothing_is_not_a_clean_run,
        ),
        (
            "an_error_result_keeps_its_kind_and_detail",
            an_error_result_keeps_its_kind_and_detail,
        ),
        (
            "the_module_does_not_inherit_the_hosts_environment",
            the_module_does_not_inherit_the_hosts_environment,
        ),
        (
            "a_broken_hello_is_an_error_not_a_run",
            a_broken_hello_is_an_error_not_a_run,
        ),
        (
            "a_program_that_does_not_exist_is_an_error",
            a_program_that_does_not_exist_is_an_error,
        ),
        (
            "the_rust_module_describes_the_five_checks",
            the_rust_module_describes_the_five_checks,
        ),
    ];
    // `cargo test <substring>` passes the substrings through; flags (`--nocapture`, ...) are ignored
    let filters: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let mut failed = Vec::new();
    let mut ran = 0;
    println!("running tests");
    for (name, test) in tests {
        if !filters.is_empty() && !filters.iter().any(|f| name.contains(f.as_str())) {
            continue;
        }
        ran += 1;
        match std::panic::catch_unwind(test) {
            Ok(()) => println!("test {name} ... ok"),
            Err(_) => {
                println!("test {name} ... FAILED");
                failed.push(*name);
            }
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed",
        if failed.is_empty() { "ok" } else { "FAILED" },
        ran - failed.len(),
        failed.len()
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
