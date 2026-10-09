//! Archive installs: a tool shipped as a `.tar.gz` or `.zip` is unpacked by CodeRipper itself, one named member only, after the
//! archive's own checksum and before the member's checksum are both checked. An archive that tries to leave the cache, or carries a
//! link, is refused whole, even when the member we want is fine.

use super::archive::{extract_member, ArchiveKind};
use super::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;

const PLATFORM: &str = "x86_64-linux";
const SRC: &str = "https://example.com/fake-1.0.tar.gz";
const TOOL: &[u8] = b"#!tool\nthe real executable\n";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Serving(HashMap<String, Vec<u8>>, std::cell::RefCell<usize>);

impl Serving {
    fn one(bytes: Vec<u8>) -> Self {
        Serving(
            HashMap::from([(SRC.to_string(), bytes)]),
            std::cell::RefCell::new(0),
        )
    }
}

impl Fetcher for Serving {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, String> {
        *self.1.borrow_mut() += 1;
        self.0.get(source).cloned().ok_or_else(|| "no".to_string())
    }
}

// ---- building archives (raw headers, so the hostile ones can be made) ----

/// A tar header with `name` written straight into it (the builder API would refuse `..`).
fn tar_header(name: &str, kind: tar::EntryType, size: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
    header.set_entry_type(kind);
    header.set_size(size);
    header.set_mode(0o755);
    header.set_cksum();
    header
}

fn tar_gz(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, kind, data) in entries {
        let mut header = tar_header(name, *kind, data.len() as u64);
        if matches!(kind, tar::EntryType::Symlink | tar::EntryType::Link) {
            header.set_link_name("target").unwrap();
            header.set_cksum();
        }
        builder.append(&header, *data).unwrap();
    }
    let raw = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&raw).unwrap();
    gz.finish().unwrap()
}

fn file(name: &str, data: &[u8]) -> (String, tar::EntryType, Vec<u8>) {
    (name.to_string(), tar::EntryType::Regular, data.to_vec())
}

fn tar_of(items: &[(String, tar::EntryType, Vec<u8>)]) -> Vec<u8> {
    let borrowed: Vec<(&str, tar::EntryType, &[u8])> = items
        .iter()
        .map(|(n, k, d)| (n.as_str(), *k, d.as_slice()))
        .collect();
    tar_gz(&borrowed)
}

fn zip_of(items: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    // (name, symlink target if this entry is a symlink, data)
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);
    for (name, link, data) in items {
        match link {
            Some(target) => writer.add_symlink(*name, *target, options).unwrap(),
            None => {
                writer.start_file(*name, options).unwrap();
                writer.write_all(data).unwrap();
            }
        }
    }
    writer.finish().unwrap().into_inner()
}

fn lock(kind: &str, member: &str, archive: &[u8], member_hash: &str) -> ToolsLock {
    ToolsLock::parse(&lock_text(kind, member, &sha(archive), member_hash)).unwrap()
}

fn lock_text(kind: &str, member: &str, archive_hash: &str, member_hash: &str) -> String {
    format!(
        "[[tool]]\nname = \"fake\"\nversion = \"1.0.0\"\nplatform = \"{PLATFORM}\"\nsource = \"{SRC}\"\nsha256 = \"{archive_hash}\"\n\
         licence = \"MIT\"\narchive = \"{kind}\"\nmember = \"{member}\"\nfile_sha256 = \"{member_hash}\"\n"
    )
}

fn install(
    lock: &ToolsLock,
    archive: Vec<u8>,
    consent: Consent,
) -> (tempfile::TempDir, Result<PathBuf, ToolError>, usize) {
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().join("tools"));
    let fetcher = Serving::one(archive);
    let result = cache.ensure(lock, "fake", PLATFORM, consent, &fetcher);
    let fetched = *fetcher.1.borrow();
    (dir, result, fetched)
}

/// Every file under `dir` (the whole temp directory: a traversal would show up beside `tools`).
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

// ---- the good paths ----

#[test]
fn a_tar_gz_tool_is_unpacked_to_the_exact_path_and_nothing_else_is() {
    let archive = tar_of(&[
        file("README.md", b"docs"),
        file("fake-1.0/bin/fake", TOOL),
        file("LICENSE", b"mit"),
    ]);
    let lock = lock("tar.gz", "fake-1.0/bin/fake", &archive, &sha(TOOL));
    let (dir, result, fetched) = install(&lock, archive, Consent::Granted);
    let path = result.unwrap();
    assert_eq!(fetched, 1);
    assert_eq!(
        path,
        dir.path()
            .join("tools")
            .join("fake")
            .join("1.0.0")
            .join("fake")
    );
    assert_eq!(std::fs::read(&path).unwrap(), TOOL);
    // only the member was written: the README and LICENSE stayed in the archive
    let written = files_under(dir.path());
    assert_eq!(written, vec![path], "{written:?}");
}

#[test]
fn a_leading_dot_slash_in_the_archive_does_not_hide_the_member() {
    let archive = tar_of(&[file("./fake", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    assert!(install(&lock, archive, Consent::Granted).1.is_ok());
}

#[test]
fn a_zip_tool_is_unpacked_the_same_way() {
    let archive = zip_of(&[("docs/x.txt", None, b"d"), ("fake.exe", None, TOOL)]);
    let lock = lock("zip", "fake.exe", &archive, &sha(TOOL));
    let (_dir, result, _) = install(&lock, archive, Consent::Granted);
    assert_eq!(std::fs::read(result.unwrap()).unwrap(), TOOL);
}

#[test]
fn an_installed_archive_tool_needs_no_consent_and_no_second_download() {
    let archive = tar_of(&[file("fake", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fetcher = Serving::one(archive);
    let path = cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fetcher)
        .unwrap();
    let again = cache
        .ensure(&lock, "fake", PLATFORM, Consent::NotGiven, &fetcher)
        .unwrap();
    assert_eq!(again, path);
    assert_eq!(*fetcher.1.borrow(), 1);
    let entry = lock.for_platform("fake", PLATFORM).unwrap();
    assert_eq!(cache.status(entry), ToolStatus::Installed);
}

#[test]
fn a_corrupt_extracted_file_is_left_alone_without_consent_and_replaced_with_it() {
    let archive = tar_of(&[file("fake", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    let dir = tempfile::tempdir().unwrap();
    let cache = ToolCache::new(dir.path().to_path_buf());
    let fetcher = Serving::one(archive);
    let path = cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fetcher)
        .unwrap();
    std::fs::write(&path, b"tampered").unwrap();
    let entry = lock.for_platform("fake", PLATFORM).unwrap();
    assert_eq!(cache.status(entry), ToolStatus::Corrupt);
    let err = cache
        .ensure(&lock, "fake", PLATFORM, Consent::NotGiven, &fetcher)
        .unwrap_err();
    assert_eq!(err.code(), "checksum_mismatch");
    assert_eq!(std::fs::read(&path).unwrap(), b"tampered");
    cache
        .ensure(&lock, "fake", PLATFORM, Consent::Granted, &fetcher)
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), TOOL);
}

// ---- checksums: both levels ----

#[test]
fn a_wrong_archive_checksum_is_refused_before_anything_is_opened() {
    let archive = tar_of(&[file("fake", TOOL)]);
    let lock = ToolsLock::parse(&lock_text("tar.gz", "fake", &"a".repeat(64), &sha(TOOL))).unwrap();
    let (dir, result, _) = install(&lock, archive, Consent::Granted);
    assert_eq!(result.unwrap_err().code(), "checksum_mismatch");
    assert!(files_under(dir.path()).is_empty());
}

#[test]
fn a_wrong_member_checksum_is_refused_and_leaves_no_file() {
    let archive = tar_of(&[file("fake", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &"b".repeat(64));
    let (dir, result, _) = install(&lock, archive, Consent::Granted);
    let err = result.unwrap_err();
    assert_eq!(err.code(), "checksum_mismatch");
    assert!(err.to_string().contains("member"), "{err}");
    assert!(files_under(dir.path()).is_empty());
}

// ---- hostile archives ----

#[test]
fn a_path_that_leaves_the_cache_refuses_the_whole_tar_even_beside_a_good_member() {
    for evil in [
        "../evil",
        "a/../../evil",
        "/etc/evil",
        "..\\evil",
        "C:\\evil",
        "C:/evil",
        "a\\b",
    ] {
        let archive = tar_of(&[file(evil, b"x"), file("fake", TOOL)]);
        let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
        let (dir, result, _) = install(&lock, archive, Consent::Granted);
        let err = result.unwrap_err();
        assert_eq!(err.code(), "archive_refused", "{evil:?}: {err}");
        assert!(files_under(dir.path()).is_empty(), "{evil:?} wrote files");
    }
}

#[test]
fn a_link_in_a_tar_refuses_the_whole_archive() {
    for kind in [tar::EntryType::Symlink, tar::EntryType::Link] {
        let archive = tar_gz(&[("fake", tar::EntryType::Regular, TOOL), ("ln", kind, b"")]);
        let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
        let (dir, result, _) = install(&lock, archive, Consent::Granted);
        assert_eq!(result.unwrap_err().code(), "archive_refused", "{kind:?}");
        assert!(files_under(dir.path()).is_empty());
    }
}

#[test]
fn a_device_or_fifo_in_a_tar_is_refused() {
    for kind in [
        tar::EntryType::Char,
        tar::EntryType::Block,
        tar::EntryType::Fifo,
    ] {
        let archive = tar_gz(&[("fake", tar::EntryType::Regular, TOOL), ("dev", kind, b"")]);
        let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
        assert_eq!(
            install(&lock, archive, Consent::Granted)
                .1
                .unwrap_err()
                .code(),
            "archive_refused",
            "{kind:?}"
        );
    }
}

#[test]
fn a_member_that_is_itself_a_link_is_refused() {
    let archive = tar_gz(&[("fake", tar::EntryType::Symlink, b"")]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    assert_eq!(
        install(&lock, archive, Consent::Granted)
            .1
            .unwrap_err()
            .code(),
        "archive_refused"
    );
}

#[test]
fn a_path_that_leaves_the_cache_refuses_the_whole_zip() {
    for evil in [
        "../evil",
        "/etc/evil",
        "..\\evil",
        "C:\\evil",
        "a/../../evil",
    ] {
        let archive = zip_of(&[(evil, None, b"x"), ("fake", None, TOOL)]);
        let lock = lock("zip", "fake", &archive, &sha(TOOL));
        let (dir, result, _) = install(&lock, archive, Consent::Granted);
        assert_eq!(result.unwrap_err().code(), "archive_refused", "{evil:?}");
        assert!(files_under(dir.path()).is_empty());
    }
}

#[test]
fn a_symlink_in_a_zip_refuses_the_whole_archive() {
    let archive = zip_of(&[("fake", None, TOOL), ("ln", Some("/etc/passwd"), b"")]);
    let lock = lock("zip", "fake", &archive, &sha(TOOL));
    let (dir, result, _) = install(&lock, archive, Consent::Granted);
    assert_eq!(result.unwrap_err().code(), "archive_refused");
    assert!(files_under(dir.path()).is_empty());
}

#[test]
fn two_entries_with_the_member_name_are_ambiguous_and_refused() {
    let archive = tar_of(&[file("fake", TOOL), file("fake", b"other")]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    assert_eq!(
        install(&lock, archive, Consent::Granted)
            .1
            .unwrap_err()
            .code(),
        "archive_refused"
    );
}

// ---- not hostile, just wrong ----

#[test]
fn a_member_the_archive_does_not_have_is_an_install_failure_that_names_it() {
    let archive = tar_of(&[file("other", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    let err = install(&lock, archive, Consent::Granted).1.unwrap_err();
    assert_eq!(err.code(), "tool_install_failed");
    assert!(err.to_string().contains("fake"), "{err}");
}

#[test]
fn bytes_that_are_not_the_declared_archive_kind_are_an_install_failure() {
    let junk = b"this is not an archive at all".to_vec();
    for kind in ["tar.gz", "zip"] {
        let lock = lock(kind, "fake", &junk, &sha(TOOL));
        let err = install(&lock, junk.clone(), Consent::Granted)
            .1
            .unwrap_err();
        assert_eq!(err.code(), "tool_install_failed", "{kind}: {err}");
    }
}

#[test]
fn no_consent_means_no_download_for_an_archive_either() {
    let archive = tar_of(&[file("fake", TOOL)]);
    let lock = lock("tar.gz", "fake", &archive, &sha(TOOL));
    let (dir, result, fetched) = install(&lock, archive, Consent::NotGiven);
    assert_eq!(result.unwrap_err().code(), "consent_not_given");
    assert_eq!(fetched, 0);
    assert!(!dir.path().join("tools").exists());
}

// ---- the size limit (the internal function, with a small limit) ----

#[test]
fn a_member_over_the_limit_is_refused_not_truncated() {
    let big = vec![7u8; 4096];
    let targz = tar_of(&[file("fake", &big)]);
    let zip = zip_of(&[("fake", None, &big)]);
    for (kind, bytes) in [(ArchiveKind::TarGz, targz), (ArchiveKind::Zip, zip)] {
        let err = extract_member(kind, &bytes, "fake", 1000).unwrap_err();
        assert!(err.to_string().contains("limit"), "{kind:?}: {err}");
        assert_eq!(
            extract_member(kind, &bytes, "fake", 4096).unwrap(),
            big,
            "{kind:?} at exactly the limit"
        );
    }
}

// ---- the lock's new fields ----

#[test]
fn the_lock_requires_archive_member_and_file_hash_together() {
    let base = |extra: &str| {
        format!(
            "[[tool]]\nname = \"fake\"\nversion = \"1.0.0\"\nplatform = \"{PLATFORM}\"\nsource = \"{SRC}\"\nsha256 = \"{}\"\nlicence = \"MIT\"\n{extra}",
            "a".repeat(64)
        )
    };
    let h = "b".repeat(64);
    for bad in [
        "archive = \"tar.gz\"\n".to_string(),
        "archive = \"tar.gz\"\nmember = \"fake\"\n".to_string(),
        format!("archive = \"tar.gz\"\nfile_sha256 = \"{h}\"\n"),
        format!("member = \"fake\"\nfile_sha256 = \"{h}\"\n"),
        format!("file_sha256 = \"{h}\"\n"),
        "member = \"fake\"\n".to_string(),
        format!("archive = \"rar\"\nmember = \"fake\"\nfile_sha256 = \"{h}\"\n"),
        format!(
            "archive = \"tar.gz\"\nmember = \"fake\"\nfile_sha256 = \"{}\"\n",
            "z".repeat(64)
        ),
        format!("archive = \"tar.gz\"\nmember = \"../x\"\nfile_sha256 = \"{h}\"\n"),
        format!("archive = \"tar.gz\"\nmember = \"/x\"\nfile_sha256 = \"{h}\"\n"),
        format!("archive = \"tar.gz\"\nmember = \"a\\\\b\"\nfile_sha256 = \"{h}\"\n"),
        format!("archive = \"tar.gz\"\nmember = \"\"\nfile_sha256 = \"{h}\"\n"),
    ] {
        let err = ToolsLock::parse(&base(&bad)).unwrap_err();
        assert_eq!(err.code(), "tools_lock_invalid", "{bad}: {err}");
    }
    // the positive control: the complete form parses
    let ok = format!("archive = \"tar.gz\"\nmember = \"d/fake\"\nfile_sha256 = \"{h}\"\n");
    assert!(ToolsLock::parse(&base(&ok)).is_ok());
    let ok = format!("archive = \"zip\"\nmember = \"fake.exe\"\nfile_sha256 = \"{h}\"\n");
    assert!(ToolsLock::parse(&base(&ok)).is_ok());
}

#[test]
fn walking_past_a_huge_entry_is_bounded_too() {
    // the wanted member is tiny, but another entry decompresses to far more than the stream allowance (4 x the limit, at least 1 MiB)
    let archive = tar_of(&[file("big", &vec![0u8; 3 << 20]), file("fake", TOOL)]);
    let err = extract_member(ArchiveKind::TarGz, &archive, "fake", 1000).unwrap_err();
    assert!(
        matches!(err, super::archive::ArchiveError::Failed(_)),
        "{err}"
    );
    // the positive control: with room for it, the same archive gives the member
    assert_eq!(
        extract_member(ArchiveKind::TarGz, &archive, "fake", 4 << 20).unwrap(),
        TOOL
    );
}

#[test]
fn a_tar_name_that_is_not_utf8_is_refused() {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.as_old_mut().name[..3].copy_from_slice(&[0xff, 0xfe, b'x']);
    header.set_size(1);
    header.set_cksum();
    builder.append(&header, &b"x"[..]).unwrap();
    let raw = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&raw).unwrap();
    let archive = gz.finish().unwrap();
    let err = extract_member(ArchiveKind::TarGz, &archive, "fake", 1000).unwrap_err();
    assert!(
        matches!(err, super::archive::ArchiveError::Refused(_)),
        "{err}"
    );
}

#[test]
fn more_hostile_names_are_refused() {
    for evil in ["..", "a/./../b", "a//../b", "a/.."] {
        let archive = tar_of(&[file(evil, b"x"), file("fake", TOOL)]);
        let err = extract_member(ArchiveKind::TarGz, &archive, "fake", 1000).unwrap_err();
        assert!(
            matches!(err, super::archive::ArchiveError::Refused(_)),
            "{evil:?}: {err}"
        );
    }
}

#[test]
fn a_member_that_is_a_directory_is_refused() {
    let archive = tar_gz(&[("fake", tar::EntryType::Directory, b"")]);
    let err = extract_member(ArchiveKind::TarGz, &archive, "fake", 1000).unwrap_err();
    assert!(
        matches!(err, super::archive::ArchiveError::Refused(_)),
        "{err}"
    );
}
