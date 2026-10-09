#![cfg(feature = "cli")]
//! `coderipper tools`: the tool cache from the command line. A tool is a file served by a local fake server (never the real
//! network), pinned in a lock the test writes. Exit 0 on success, 3 when a tool could not be had (no consent, bad checksum, no
//! download), 2 for a usage error.

use assert_cmd::Command;
use predicates::prelude::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Serves `body` at any path to every connection, for as long as the test process lives; returns the base URL.
fn serve(body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn platform() -> String {
    coderipper::tools::current_platform()
}

fn write_lock(dir: &Path, source: &str, hash: &str) -> std::path::PathBuf {
    let lock = dir.join("tools.lock");
    std::fs::write(
        &lock,
        format!(
            "[[tool]]\nname = \"faketool\"\nversion = \"1.2.3\"\nplatform = \"{}\"\nsource = \"{source}\"\nsha256 = \"{hash}\"\nlicence = \"Apache-2.0\"\n",
            platform()
        ),
    )
    .unwrap();
    lock
}

fn coderipper(tools_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("coderipper").unwrap();
    cmd.env("CODERIPPER_TOOLS_DIR", tools_dir);
    cmd
}

#[test]
fn list_on_the_shipped_lock_says_nothing_is_pinned_yet() {
    let tools = tempfile::tempdir().unwrap();
    coderipper(tools.path())
        .args(["tools", "list"])
        .assert()
        .code(0)
        .stdout(predicate::str::contains("no tools are pinned yet"));
}

#[test]
fn install_without_the_flag_installs_nothing_and_prints_the_command() {
    let base = serve(b"fake tool bytes");
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let lock = write_lock(
        work.path(),
        &format!("{base}/faketool"),
        &sha(b"fake tool bytes"),
    );
    coderipper(tools.path())
        .args(["tools", "install", "faketool", "--tools-lock"])
        .arg(&lock)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("consent_not_given"))
        .stderr(predicate::str::contains(
            "coderipper tools install faketool --install-tools --tools-lock",
        ));
    assert_eq!(std::fs::read_dir(tools.path()).unwrap().count(), 0);
}

#[test]
fn install_with_the_flag_installs_and_list_shows_the_ledger() {
    let base = serve(b"fake tool bytes");
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let lock = write_lock(
        work.path(),
        &format!("{base}/faketool"),
        &sha(b"fake tool bytes"),
    );
    coderipper(tools.path())
        .args(["tools", "list", "--tools-lock"])
        .arg(&lock)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("faketool"))
        .stdout(predicate::str::contains("1.2.3"))
        .stdout(predicate::str::contains("Apache-2.0"))
        .stdout(predicate::str::contains("not installed"));
    coderipper(tools.path())
        .args([
            "tools",
            "install",
            "faketool",
            "--install-tools",
            "--tools-lock",
        ])
        .arg(&lock)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("faketool 1.2.3"));
    let exe = if cfg!(windows) {
        "faketool.exe"
    } else {
        "faketool"
    };
    let installed = tools.path().join("faketool").join("1.2.3").join(exe);
    assert_eq!(std::fs::read(&installed).unwrap(), b"fake tool bytes");
    coderipper(tools.path())
        .args(["tools", "list", "--tools-lock"])
        .arg(&lock)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("installed"))
        .stdout(predicate::str::contains("not installed").not());
}

#[test]
fn a_wrong_checksum_is_reported_as_such_and_leaves_no_file() {
    let base = serve(b"something else");
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let lock = write_lock(
        work.path(),
        &format!("{base}/faketool"),
        &sha(b"fake tool bytes"),
    );
    coderipper(tools.path())
        .args([
            "tools",
            "install",
            "faketool",
            "--install-tools",
            "--tools-lock",
        ])
        .arg(&lock)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("checksum_mismatch"));
    assert_eq!(std::fs::read_dir(tools.path()).unwrap().count(), 0);
}

#[test]
fn an_unknown_tool_is_tool_missing() {
    let base = serve(b"x");
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let lock = write_lock(work.path(), &format!("{base}/x"), &sha(b"x"));
    coderipper(tools.path())
        .args([
            "tools",
            "install",
            "nosuch",
            "--install-tools",
            "--tools-lock",
        ])
        .arg(&lock)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("tool_missing"));
}

#[test]
fn a_broken_lock_is_a_reported_error_not_an_empty_list() {
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let lock = work.path().join("tools.lock");
    std::fs::write(&lock, "[[tool]]\nname = \"x\"\n").unwrap();
    coderipper(tools.path())
        .args(["tools", "list", "--tools-lock"])
        .arg(&lock)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("tools_lock_invalid"));
}

#[test]
fn a_file_source_installs_from_disk() {
    let work = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let blob = work.path().join("blob");
    std::fs::write(&blob, b"mirrored").unwrap();
    let url = format!(
        "file:///{}",
        blob.to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    // On unix the path keeps its leading slash.
    let url = if cfg!(windows) {
        url
    } else {
        format!("file://{}", blob.to_string_lossy())
    };
    let lock = write_lock(work.path(), &url, &sha(b"mirrored"));
    coderipper(tools.path())
        .args(["tools", "install", "--install-tools", "--tools-lock"])
        .arg(&lock)
        .assert()
        .code(0)
        .stdout(predicate::str::contains("faketool 1.2.3"));
}
