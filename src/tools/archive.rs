//! Reading one executable out of a downloaded archive, without trusting the archive.
//!
//! The rule is strict on purpose: an archive is refused **whole** when *any* entry has a name that could leave the directory it is
//! unpacked into (`..`, an absolute path, a drive, a backslash) or is anything but a file or a directory (symlink, hardlink, device,
//! fifo), even if the member we want is fine. We only ever write the one member, by a path the lock chose, so those entries could not
//! do harm here; refusing them anyway means a release that starts to ship one is noticed at the lock review instead of passing
//! because of a detail of how we unpack. (A zip whose central directory lists one name twice is read by the zip crate as one entry,
//! the last; the member's hash still binds what is written, but the duplicate is not reported.) Entries are read in memory and nothing is written until the whole archive has passed.

use serde::Deserialize;
use std::io::Read;

/// How a tool's download is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ArchiveKind {
    /// A gzip-compressed tar.
    #[serde(rename = "tar.gz")]
    TarGz,
    /// A zip.
    #[serde(rename = "zip")]
    Zip,
}

/// Why the member could not be had.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(super) enum ArchiveError {
    /// The archive has an entry CodeRipper will not unpack.
    #[error("{0}")]
    Refused(String),
    /// The archive is unreadable, or lacks the member, or the member is too big.
    #[error("{0}")]
    Failed(String),
}

/// Why `name` could not be a safe relative path inside a directory, if it could not.
fn unsafe_name(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("an entry has an empty name");
    }
    if name.contains(['\\', '\0']) {
        return Some("a name contains a backslash or a NUL");
    }
    if name.starts_with('/') {
        return Some("a name is an absolute path");
    }
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some("a name starts with a drive");
    }
    if name.split('/').any(|part| part == "..") {
        return Some("a name climbs out with `..`");
    }
    None
}

/// The entry's name as the lock spells a member: no leading `./`, no trailing `/`, no empty or `.` parts.
fn normalised(name: &str) -> String {
    name.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn refuse(what: &str, name: &str) -> ArchiveError {
    ArchiveError::Refused(format!("{what}: {name:?}"))
}

/// The bytes of `member` from `archive`, after checking every entry. `limit` caps the member's size.
pub(super) fn extract_member(
    kind: ArchiveKind,
    archive: &[u8],
    member: &str,
    limit: u64,
) -> Result<Vec<u8>, ArchiveError> {
    match kind {
        ArchiveKind::TarGz => from_tar_gz(archive, member, limit),
        ArchiveKind::Zip => from_zip(archive, member, limit),
    }
}

fn read_member(reader: impl Read, limit: u64, member: &str) -> Result<Vec<u8>, ArchiveError> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| ArchiveError::Failed(format!("cannot read {member:?}: {e}")))?;
    if bytes.len() as u64 > limit {
        return Err(ArchiveError::Failed(format!(
            "{member:?} is over the {limit}-byte limit"
        )));
    }
    Ok(bytes)
}

fn from_tar_gz(archive: &[u8], member: &str, limit: u64) -> Result<Vec<u8>, ArchiveError> {
    use tar::EntryType;
    // Walking past entries decompresses them: bound the whole stream, not only the member.
    let stream_limit = limit.saturating_mul(4).max(1 << 20);
    let consumed = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let decoder = Counting {
        inner: flate2::read::GzDecoder::new(archive).take(stream_limit),
        count: consumed.clone(),
    };
    let mut tar = tar::Archive::new(decoder);
    let unreadable = |e: std::io::Error| ArchiveError::Failed(format!("unreadable tar.gz: {e}"));
    let mut found: Option<Vec<u8>> = None;
    for entry in tar.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        let raw = entry.path_bytes().into_owned();
        let name = std::str::from_utf8(&raw)
            .map_err(|_| ArchiveError::Refused("a name is not valid UTF-8".into()))?;
        if let Some(why) = unsafe_name(name) {
            return Err(refuse(why, name));
        }
        let kind = entry.header().entry_type();
        let is_file = matches!(kind, EntryType::Regular | EntryType::Continuous);
        match kind {
            EntryType::Regular | EntryType::Continuous | EntryType::Directory => {}
            // metadata records the reader normally consumes itself; they write nothing
            EntryType::XHeader
            | EntryType::XGlobalHeader
            | EntryType::GNULongName
            | EntryType::GNULongLink => continue,
            _ => {
                return Err(refuse(
                    "an entry is a link, device or other special file",
                    name,
                ))
            }
        }
        if normalised(name) == member {
            if !is_file {
                return Err(refuse("the member is not a regular file", name));
            }
            if found.is_some() {
                return Err(refuse("two entries have the member's name", name));
            }
            found = Some(read_member(&mut entry, limit, member)?);
        }
    }
    // A stream cut at the allowance can end exactly on an entry boundary, which the tar reader takes for a clean end and which
    // would leave later entries unchecked: reaching the allowance is an error whatever the reader saw.
    if consumed.get() >= stream_limit {
        return Err(ArchiveError::Failed(format!(
            "the archive unpacks to more than {stream_limit} bytes, so its later entries cannot be checked"
        )));
    }
    found.ok_or_else(|| ArchiveError::Failed(format!("the archive has no member {member:?}")))
}

/// Counts the bytes that pass through a reader.
struct Counting<R> {
    inner: R,
    count: std::rc::Rc<std::cell::Cell<u64>>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.set(self.count.get() + n as u64);
        Ok(n)
    }
}

fn from_zip(archive: &[u8], member: &str, limit: u64) -> Result<Vec<u8>, ArchiveError> {
    let unreadable =
        |e: zip::result::ZipError| ArchiveError::Failed(format!("unreadable zip: {e}"));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(unreadable)?;
    let mut found: Option<usize> = None;
    for index in 0..zip.len() {
        // `_raw`: names and modes only; nothing is decompressed (or decrypted) while checking
        let entry = zip.by_index_raw(index).map_err(unreadable)?;
        let name = entry
            .name()
            .map_err(|_| ArchiveError::Refused("a name is not valid UTF-8".into()))?
            .into_owned();
        if let Some(why) = unsafe_name(&name) {
            return Err(refuse(why, &name));
        }
        // S_IFMT: 0 (unknown, as written on Windows), a regular file or a directory are acceptable; a link or device is not
        let file_type = entry.unix_mode().map_or(0, |m| m & 0o170000);
        if entry.is_symlink() || !matches!(file_type, 0 | 0o100000 | 0o040000) {
            return Err(refuse(
                "an entry is a link, device or other special file",
                &name,
            ));
        }
        if normalised(&name) == member {
            if entry.is_dir() {
                return Err(refuse("the member is not a regular file", &name));
            }
            if found.is_some() {
                return Err(refuse("two entries have the member's name", &name));
            }
            found = Some(index);
        }
    }
    let index = found
        .ok_or_else(|| ArchiveError::Failed(format!("the archive has no member {member:?}")))?;
    let entry = zip.by_index(index).map_err(unreadable)?;
    if entry.size() > limit {
        return Err(ArchiveError::Failed(format!(
            "{member:?} is over the {limit}-byte limit"
        )));
    }
    read_member(entry, limit, member)
}
