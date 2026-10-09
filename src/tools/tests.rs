use super::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;

const PLATFORM: &str = "x86_64-linux";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn lock_text(name: &str, version: &str, platform: &str, source: &str, hash: &str) -> String {
    format!(
        "[[tool]]\nname = \"{name}\"\nversion = \"{version}\"\nplatform = \"{platform}\"\nsource = \"{source}\"\nsha256 = \"{hash}\"\nlicence = \"MIT\"\n"
    )
}

/// A fetcher that serves canned bytes and counts how often it was asked.
struct Fake {
    files: HashMap<String, Vec<u8>>,
    calls: RefCell<Vec<String>>,
}

impl Fake {
    fn serving(source: &str, bytes: &[u8]) -> Self {
        Fake {
            files: HashMap::from([(source.to_string(), bytes.to_vec())]),
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl Fetcher for Fake {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, String> {
        self.calls.borrow_mut().push(source.to_string());
        self.files
            .get(source)
            .cloned()
            .ok_or_else(|| format!("no such source {source}"))
    }
}

const SRC: &str = "https://example.com/fake-1.0";

fn lock_for(bytes: &[u8]) -> ToolsLock {
    ToolsLock::parse(&lock_text("fake", "1.0.0", PLATFORM, SRC, &sha(bytes))).unwrap()
}

// ---- the lock ----

#[test]
fn a_valid_lock_parses_and_finds_its_entry() {
    let lock = lock_for(b"x");
    let entry = lock.for_platform("fake", PLATFORM).unwrap();
    assert_eq!(entry.version, "1.0.0");
    assert!(lock.for_platform("fake", "aarch64-macos").is_none());
    assert!(lock.knows("fake") && !lock.knows("other"));
}

#[test]
fn the_embedded_lock_parses() {
    ToolsLock::embedded();
}

#[test]
fn a_lock_rejects_a_malformed_checksum() {
    for bad in ["abc", &"g".repeat(64), &"a".repeat(63)] {
        let err = ToolsLock::parse(&lock_text("fake", "1.0.0", PLATFORM, SRC, bad)).unwrap_err();
        assert_eq!(err.code(), "tools_lock_invalid", "{bad}: {err}");
    }
}

#[test]
fn a_lock_rejects_names_that_could_leave_the_cache() {
    let hash = "a".repeat(64);
    for bad in [
        "..", "a/b", "a\\b", "", ".hidden", "con", "NUL.exe", "com1", "x.", "a b",
    ] {
        let err = ToolsLock::parse(&lock_text(bad, "1.0.0", PLATFORM, SRC, &hash)).unwrap_err();
        assert_eq!(err.code(), "tools_lock_invalid", "name {bad:?}: {err}");
        let err = ToolsLock::parse(&lock_text("fake", bad, PLATFORM, SRC, &hash)).unwrap_err();
        assert_eq!(err.code(), "tools_lock_invalid", "version {bad:?}: {err}");
    }
}

#[test]
fn a_lock_rejects_a_file_name_that_is_a_path() {
    let text = format!(
        "{}file = \"../evil\"\n",
        lock_text("fake", "1.0.0", PLATFORM, SRC, &"a".repeat(64))
    );
    assert_eq!(
        ToolsLock::parse(&text).unwrap_err().code(),
        "tools_lock_invalid"
    );
}

#[test]
fn a_lock_accepts_only_https_loopback_http_and_file_sources() {
    let hash = "a".repeat(64);
    for good in [
        "https://example.com/x",
        "http://127.0.0.1:9/x",
        "http://localhost/x",
        "http://[::1]:8080/x",
        "file:///tmp/x",
        "file://localhost/tmp/x",
        "file:///C:/tools/x",
    ] {
        ToolsLock::parse(&lock_text("fake", "1.0.0", PLATFORM, good, &hash))
            .unwrap_or_else(|e| panic!("{good}: {e}"));
    }
    for bad in [
        "http://example.com/x",
        "ftp://example.com/x",
        "example.com/x",
        "http://127.0.0.1.evil.com/x",
        "http://localhost:80@evil.com/x",
        "http://localhost:@evil.com/",
        "http://[::1]@evil.com/",
        "http://127.0.0.1@evil.com/",
        "http://127.0.0.1:80x/x",
        r"http://127.0.0.1\@evil.com/",
        "https://",
        "file://relative/x",
        "file://server/share/x",
        "file://",
    ] {
        let err = ToolsLock::parse(&lock_text("fake", "1.0.0", PLATFORM, bad, &hash)).unwrap_err();
        assert_eq!(err.code(), "tools_lock_invalid", "{bad}: {err}");
    }
}

#[test]
fn a_lock_rejects_two_versions_of_one_tool_on_one_platform() {
    let hash = "a".repeat(64);
    let text = format!(
        "{}{}",
        lock_text("fake", "1.0.0", PLATFORM, SRC, &hash),
        lock_text("fake", "2.0.0", PLATFORM, SRC, &hash)
    );
    assert_eq!(
        ToolsLock::parse(&text).unwrap_err().code(),
        "tools_lock_invalid"
    );
}

#[test]
fn a_lock_rejects_an_unknown_field() {
    let text = format!(
        "{}extra = 1\n",
        lock_text("fake", "1.0.0", PLATFORM, SRC, &"a".repeat(64))
    );
    assert_eq!(
        ToolsLock::parse(&text).unwrap_err().code(),
        "tools_lock_invalid"
    );
}

// ---- where the cache lives ----

fn abs(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap()
}

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(key, _)| *key == k)
            .map(|(_, v)| v.to_string())
    }
}

#[test]
fn the_tools_dir_is_the_explicit_variable_first() {
    let get = env(&[
        ("CODERIPPER_TOOLS_DIR", "/t"),
        ("CODERIPPER_CACHE_DIR", "/c/build"),
    ]);
    assert_eq!(tools_dir_from_env(&get), Some(abs("/t")));
}

#[test]
fn the_tools_dir_sits_beside_the_build_cache() {
    let get = env(&[("CODERIPPER_CACHE_DIR", "/c/build")]);
    assert_eq!(tools_dir_from_env(&get), Some(abs("/c/tools")));
    let get = env(&[("LOCALAPPDATA", "/local")]);
    assert_eq!(
        tools_dir_from_env(&get),
        Some(abs("/local/coderipper/tools"))
    );
    let get = env(&[("XDG_CACHE_HOME", "/xdg")]);
    assert_eq!(tools_dir_from_env(&get), Some(abs("/xdg/coderipper/tools")));
    let get = env(&[("HOME", "/home/u")]);
    assert_eq!(
        tools_dir_from_env(&get),
        Some(abs("/home/u/.cache/coderipper/tools"))
    );
    assert_eq!(tools_dir_from_env(&env(&[])), None);
}

#[test]
fn an_empty_variable_counts_as_unset() {
    let get = env(&[("CODERIPPER_TOOLS_DIR", " "), ("HOME", "/h")]);
    assert_eq!(
        tools_dir_from_env(&get),
        Some(abs("/h/.cache/coderipper/tools"))
    );
}

// ---- installing ----

#[test]
fn nothing_is_fetched_or_written_without_consent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving(SRC, b"binary");
    let err = cache
        .ensure(
            &lock_for(b"binary"),
            "fake",
            PLATFORM,
            Consent::NotGiven,
            &fake,
        )
        .unwrap_err();
    assert_eq!(err.code(), "consent_not_given");
    assert!(
        err.to_string()
            .contains("coderipper tools install fake --install-tools"),
        "{err}"
    );
    assert!(fake.calls.borrow().is_empty(), "fetched without consent");
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "wrote without consent"
    );
}

#[test]
fn consent_installs_to_the_exact_path_and_a_second_ensure_fetches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving(SRC, b"binary");
    let lock = lock_for(b"binary");
    let path = cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fake)
        .unwrap();
    assert_eq!(path, dir.path().join("fake").join("1.0.0").join("fake"));
    assert_eq!(std::fs::read(&path).unwrap(), b"binary");
    // The same version again, and without consent: already there, so no consent is needed and nothing is fetched.
    let again = cache
        .ensure(&lock, "fake", PLATFORM, Consent::NotGiven, &fake)
        .unwrap();
    assert_eq!(again, path);
    assert_eq!(fake.calls.borrow().len(), 1);
}

#[test]
fn a_windows_tool_gets_an_exe_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let lock = ToolsLock::parse(&lock_text(
        "fake",
        "1.0.0",
        "x86_64-windows",
        SRC,
        &sha(b"b"),
    ))
    .unwrap();
    let path = cache
        .ensure(
            &lock,
            "fake",
            "x86_64-windows",
            Consent::Granted,
            &Fake::serving(SRC, b"b"),
        )
        .unwrap();
    assert_eq!(path.file_name().unwrap(), "fake.exe");
}

#[test]
fn a_checksum_mismatch_leaves_nothing_behind_and_is_not_reported_as_absent() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving(SRC, b"tampered");
    let err = cache
        .ensure(
            &lock_for(b"binary"),
            "fake",
            PLATFORM,
            Consent::Granted,
            &fake,
        )
        .unwrap_err();
    assert_eq!(err.code(), "checksum_mismatch");
    assert!(err.to_string().contains(&sha(b"binary")), "{err}");
    assert!(err.to_string().contains(&sha(b"tampered")), "{err}");
    let leftovers: Vec<_> = walk(dir.path());
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn a_corrupt_install_is_left_alone_without_consent_and_replaced_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving(SRC, b"binary");
    let lock = lock_for(b"binary");
    let path = cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fake)
        .unwrap();
    std::fs::write(&path, b"swapped").unwrap();
    // No consent: reported, and not touched (deleting is a write).
    let err = cache
        .ensure(&lock, "fake", PLATFORM, Consent::NotGiven, &fake)
        .unwrap_err();
    assert_eq!(err.code(), "checksum_mismatch");
    assert!(err.to_string().contains("left as it is"), "{err}");
    assert_eq!(std::fs::read(&path).unwrap(), b"swapped");
    assert_eq!(fake.calls.borrow().len(), 1, "fetched without consent");
    // Consent: replaced in one call.
    cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fake)
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"binary");
    assert_eq!(fake.calls.borrow().len(), 2);
}

#[test]
fn a_tool_not_in_the_lock_is_missing_and_one_without_this_platform_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving(SRC, b"binary");
    let lock = lock_for(b"binary");
    let missing = cache
        .ensure(&lock, "other", PLATFORM, Consent::Granted, &fake)
        .unwrap_err();
    assert_eq!(missing.code(), "tool_missing");
    let unavailable = cache
        .ensure(&lock, "fake", "aarch64-macos", Consent::Granted, &fake)
        .unwrap_err();
    assert_eq!(unavailable.code(), "tool_unavailable_for_platform");
    assert!(fake.calls.borrow().is_empty());
}

#[test]
fn a_failed_download_is_its_own_error() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fake = Fake::serving("https://elsewhere/x", b"");
    let err = cache
        .ensure(
            &lock_for(b"binary"),
            "fake",
            PLATFORM,
            Consent::Granted,
            &fake,
        )
        .unwrap_err();
    assert_eq!(err.code(), "download_failed");
    assert!(walk(dir.path()).is_empty());
}

#[test]
fn status_tells_installed_from_absent_from_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let lock = lock_for(b"binary");
    let entry = lock.for_platform("fake", PLATFORM).unwrap();
    assert_eq!(cache.status(entry), ToolStatus::NotInstalled);
    let path = cache
        .ensure(
            &lock,
            "fake",
            PLATFORM,
            Consent::Granted,
            &Fake::serving(SRC, b"binary"),
        )
        .unwrap();
    assert_eq!(cache.status(entry), ToolStatus::Installed);
    std::fs::write(&path, b"swapped").unwrap();
    assert_eq!(cache.status(entry), ToolStatus::Corrupt);
    // Reading the status never deletes: only an install or a use does.
    assert!(path.exists());
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let inner = walk(&path);
            if inner.is_empty() {
                out.push(path);
            }
            out.extend(inner);
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn a_relative_tools_dir_becomes_absolute() {
    let get = env(&[("CODERIPPER_TOOLS_DIR", "rel/tools")]);
    assert!(tools_dir_from_env(&get).unwrap().is_absolute());
    let get = env(&[("CODERIPPER_CACHE_DIR", "rel/build")]);
    let dir = tools_dir_from_env(&get).unwrap();
    assert!(dir.is_absolute() && dir.ends_with("rel/tools"), "{dir:?}");
}

#[test]
fn names_are_unique_ignoring_case() {
    let hash = "a".repeat(64);
    let text = format!(
        "{}{}",
        lock_text("Fake", "1.0.0", PLATFORM, SRC, &hash),
        lock_text("fake", "1.0.0", PLATFORM, SRC, &hash)
    );
    assert_eq!(
        ToolsLock::parse(&text).unwrap_err().code(),
        "tools_lock_invalid"
    );
}

#[test]
fn an_uppercase_checksum_in_the_lock_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let lock = ToolsLock::parse(&lock_text(
        "fake",
        "1.0.0",
        PLATFORM,
        SRC,
        &sha(b"binary").to_ascii_uppercase(),
    ))
    .unwrap();
    cache
        .ensure(
            &lock,
            "fake",
            PLATFORM,
            Consent::Granted,
            &Fake::serving(SRC, b"binary"),
        )
        .unwrap();
}

#[test]
fn the_default_fetcher_reads_file_sources_and_refuses_others() {
    let dir = tempfile::tempdir().unwrap();
    let blob = dir.path().join("blob");
    std::fs::write(&blob, b"bytes").unwrap();
    let slashed = blob.to_string_lossy().replace('\\', "/");
    let url = format!("file:///{}", slashed.trim_start_matches('/'));
    let url = if cfg!(windows) {
        url
    } else {
        format!("file://{slashed}")
    };
    assert_eq!(DefaultFetcher.fetch(&url).unwrap(), b"bytes");
    assert!(DefaultFetcher.fetch("file://relative/x").is_err());
    assert!(DefaultFetcher.fetch(&format!("{url}.missing")).is_err());
}
