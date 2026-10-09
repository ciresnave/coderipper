//! The tool cache: where the external tools a module delegates to are installed, and the policy for installing them
//! (design section 8).
//!
//! - **Isolated.** A tool lives at `<tools dir>/<tool>/<version>/<file>` (an absolute path; callers run it by that path). Nothing is
//!   written to a `PATH`, a global npm prefix, `~/.cargo/bin`, site-packages or any system location. The tools directory is `CODERIPPER_TOOLS_DIR`, else
//!   `tools` beside the build cache (see [`tools_dir_from_env`]).
//! - **Pinned.** `tools.lock` names, per tool and platform, one version, where to get it, its SHA-256 and its SPDX licence.
//!   There is no "latest": changing a tool is a reviewed change to the lock.
//! - **Checked.** A download whose SHA-256 differs is never written; the error is `checksum_mismatch`, not "tool absent".
//!   An installed file is re-hashed each time it is resolved. A file that no longer matches is replaced when consent is given, and
//!   otherwise left untouched and reported (a write, even a delete, needs consent).
//! - **Consented.** Nothing is fetched, written or deleted unless the caller passes [`Consent::Granted`] (the CLI's `--install-tools`).
//!   Without it a missing tool is `consent_not_given`, and the message prints the exact command that would install it. There is
//!   no environment or CI shortcut to consent: the only inputs are the flag and, later, the user's own configuration file.
//!
//! Each tool ends up as one executable file. The lock entry either points at that file (a checksummed prebuilt binary, or a file
//! already on disk through a `file://` source, which is also how an offline mirror works), or at a `.tar.gz` or `.zip` that holds
//! it: then the entry names the one `member` to take out and carries two hashes, the archive's (what the publisher's checksum
//! file lists) and the member's (`file_sha256`, what is re-checked every time the tool is resolved). Archives are unpacked by
//! CodeRipper itself, in memory, and refused whole if any entry could leave the directory or is a link.
//! It does not run `cargo install`, `npm` or `pip`: those installers land with the first delegation that needs them.
//! Downloads follow redirects (release hosts redirect); the checksum, not the route, is what makes the bytes trustworthy.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

mod archive;
pub use archive::ArchiveKind;

/// One line of the lock: a tool, at one version, for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ToolEntry {
    /// The tool's name; also the default file name.
    pub name: String,
    /// The one version the lock pins.
    pub version: String,
    /// `<arch>-<os>`, for example `x86_64-windows` (see [`current_platform`]).
    pub platform: String,
    /// Where the file comes from: an `https://` URL, an `http://` URL on this machine, or a `file://` path.
    pub source: String,
    /// The SHA-256 of what `source` serves (the archive, when there is one), 64 hex digits.
    pub sha256: String,
    /// The tool's SPDX licence expression, for the ledger (`coderipper tools list`).
    pub licence: String,
    /// The file name inside the version directory, when it is not the tool's name (a `.exe` suffix is added on Windows).
    #[serde(default)]
    pub file: Option<String>,
    /// How `source` is packed, when it is an archive rather than the executable itself. Needs `member` and `file_sha256`.
    #[serde(default)]
    pub archive: Option<ArchiveKind>,
    /// The path of the executable inside the archive, `/`-separated.
    #[serde(default)]
    pub member: Option<String>,
    /// The SHA-256 of the extracted member: the installed file is checked against this, not against `sha256`.
    #[serde(default)]
    pub file_sha256: Option<String>,
}

impl ToolEntry {
    /// The hash the installed file must have: the member's for an archive, else the download's.
    fn installed_sha256(&self) -> &str {
        self.file_sha256.as_deref().unwrap_or(&self.sha256)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLock {
    #[serde(default)]
    tool: Vec<ToolEntry>,
}

/// What can go wrong with a tool. [`ToolError::code`] is the stable word a script can match on.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolError {
    /// The lock file itself is wrong.
    #[error("tools_lock_invalid: {0}")]
    LockInvalid(String),
    /// The lock has no entry for this tool at all.
    #[error("tool_missing: {0} is not in the tools lock")]
    Missing(String),
    /// The lock has the tool, but not for this platform.
    #[error("tool_unavailable_for_platform: {tool} has no entry for {platform} in the tools lock")]
    UnavailableForPlatform {
        /// The tool.
        tool: String,
        /// The platform asked for.
        platform: String,
    },
    /// The tool is not installed and the user has not said it may be installed.
    #[error(
        "consent_not_given: {tool} {version} is not installed, and CodeRipper installs nothing without your consent; \
         to install it run: coderipper tools install {tool} --install-tools"
    )]
    ConsentNotGiven {
        /// The tool.
        tool: String,
        /// Its pinned version.
        version: String,
    },
    /// The bytes do not hash to what the lock says. Nothing of them is kept.
    #[error(
        "checksum_mismatch: {tool} {version}: the lock says sha256 {expected}, the file has {actual}; {note}"
    )]
    ChecksumMismatch {
        /// The tool.
        tool: String,
        /// Its pinned version.
        version: String,
        /// The hash in the lock.
        expected: String,
        /// The hash of what was found.
        actual: String,
        /// What happened to the bytes.
        note: &'static str,
    },
    /// The download failed.
    #[error("download_failed: {tool} {version} from {source_url}: {reason}")]
    DownloadFailed {
        /// The tool.
        tool: String,
        /// Its pinned version.
        version: String,
        /// The lock's source.
        source_url: String,
        /// Why.
        reason: String,
    },
    /// The archive has an entry CodeRipper will not unpack (a path that leaves the directory, a link). Nothing was written.
    #[error("archive_refused: {tool} {version}: {reason}")]
    ArchiveRefused {
        /// The tool.
        tool: String,
        /// Its pinned version.
        version: String,
        /// What was found.
        reason: String,
    },
    /// Writing into the cache failed.
    #[error("tool_install_failed: {tool} {version}: {reason}")]
    InstallFailed {
        /// The tool.
        tool: String,
        /// Its pinned version.
        version: String,
        /// Why.
        reason: String,
    },
}

impl ToolError {
    /// The stable error code (the word before the first colon of the message).
    pub fn code(&self) -> &'static str {
        match self {
            ToolError::LockInvalid(_) => "tools_lock_invalid",
            ToolError::Missing(_) => "tool_missing",
            ToolError::UnavailableForPlatform { .. } => "tool_unavailable_for_platform",
            ToolError::ConsentNotGiven { .. } => "consent_not_given",
            ToolError::ChecksumMismatch { .. } => "checksum_mismatch",
            ToolError::DownloadFailed { .. } => "download_failed",
            ToolError::ArchiveRefused { .. } => "archive_refused",
            ToolError::InstallFailed { .. } => "tool_install_failed",
        }
    }
}

/// The lock: every tool CodeRipper may install, pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsLock {
    entries: Vec<ToolEntry>,
}

impl ToolsLock {
    /// Parses and validates a lock. Every name that becomes a path component is checked here, so a lock cannot place a file
    /// outside the cache.
    pub fn parse(text: &str) -> Result<Self, ToolError> {
        let raw: RawLock =
            toml::from_str(text).map_err(|e| ToolError::LockInvalid(e.to_string()))?;
        // Compared case-insensitively: `Fake` and `fake` are one directory on Windows and macOS.
        let mut seen: Vec<(String, String)> = Vec::new();
        for entry in &raw.tool {
            validate_entry(entry)?;
            let key = (entry.name.to_ascii_lowercase(), entry.platform.clone());
            if seen.contains(&key) {
                return Err(ToolError::LockInvalid(format!(
                    "{} has two entries for {}; the lock pins one version per tool and platform",
                    entry.name, entry.platform
                )));
            }
            seen.push(key);
        }
        Ok(ToolsLock { entries: raw.tool })
    }

    /// The lock that ships inside this build (`src/tools/tools.lock`).
    pub fn embedded() -> Self {
        ToolsLock::parse(include_str!("tools.lock"))
            .unwrap_or_else(|e| panic!("the shipped tools.lock is invalid: {e}"))
    }

    /// Every entry, in file order.
    pub fn entries(&self) -> &[ToolEntry] {
        &self.entries
    }

    /// The entry for `name` on `platform`.
    pub fn for_platform(&self, name: &str, platform: &str) -> Option<&ToolEntry> {
        self.entries
            .iter()
            .find(|e| e.name == name && e.platform == platform)
    }

    /// Whether the lock has `name` for any platform.
    pub fn knows(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.name == name)
    }
}

fn invalid(entry: &ToolEntry, what: &str) -> ToolError {
    ToolError::LockInvalid(format!("{} {}: {what}", entry.name, entry.version))
}

fn validate_entry(entry: &ToolEntry) -> Result<(), ToolError> {
    for (label, value) in [("name", &entry.name), ("version", &entry.version)] {
        if !is_safe_component(value) {
            return Err(invalid(
                entry,
                &format!("the {label} {value:?} is not a safe path component"),
            ));
        }
    }
    if let Some(file) = &entry.file {
        if !is_safe_component(file) {
            return Err(invalid(
                entry,
                &format!("the file name {file:?} is not a safe path component"),
            ));
        }
    }
    if entry.platform.is_empty()
        || !entry
            .platform
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(invalid(
            entry,
            &format!("the platform {:?} is not <arch>-<os>", entry.platform),
        ));
    }
    if !is_sha256(&entry.sha256) {
        return Err(invalid(entry, "sha256 must be 64 hex digits"));
    }
    match (&entry.archive, &entry.member, &entry.file_sha256) {
        (None, None, None) => {}
        (Some(_), Some(member), Some(hash)) => {
            if member.is_empty() || !member.split('/').all(is_safe_component) {
                return Err(invalid(
                    entry,
                    &format!(
                        "the member {member:?} is not a plain relative path of safe components"
                    ),
                ));
            }
            if !is_sha256(hash) {
                return Err(invalid(entry, "file_sha256 must be 64 hex digits"));
            }
        }
        _ => {
            return Err(invalid(
                entry,
                "archive, member and file_sha256 go together: all three or none",
            ))
        }
    }
    if entry.licence.trim().is_empty() {
        return Err(invalid(entry, "a licence (SPDX expression) is required"));
    }
    if !is_allowed_source(&entry.source) {
        return Err(invalid(
            entry,
            &format!(
                "the source {:?} must be https://, http:// on this machine, or file://",
                entry.source
            ),
        ));
    }
    Ok(())
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// A name that is one plain path component on every platform CodeRipper runs on.
fn is_safe_component(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('.')
        || name.ends_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    {
        return false;
    }
    // Reserved device names, with or without an extension, are not files on Windows.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    let reserved = matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    !reserved
}

fn is_allowed_source(source: &str) -> bool {
    if source.starts_with("https://") && source.len() > "https://".len() {
        return true;
    }
    if let Some(rest) = source.strip_prefix("file://") {
        return file_source_path(rest).is_some();
    }
    if let Some(rest) = source.strip_prefix("http://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        // Userinfo (`a@b`), backslashes and whitespace make the host ambiguous between parsers: refuse them outright.
        if authority.contains(['@', '\\']) || authority.contains(char::is_whitespace) {
            return false;
        }
        let (host, port) = match authority.strip_prefix('[') {
            Some(v6) => match v6.split_once(']') {
                Some((host, "")) => (host, None),
                Some((host, tail)) => (host, tail.strip_prefix(':')),
                None => return false,
            },
            None => match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            },
        };
        let port_ok = port.is_none_or(|p| p.chars().all(|c| c.is_ascii_digit()));
        return port_ok && matches!(host, "127.0.0.1" | "localhost" | "::1");
    }
    false
}

/// The local path a `file://` source names: `file:///abs/path` or `file://localhost/abs/path` (and `file:///C:/x` on Windows).
/// A relative path or another host is not a local file. There is no percent-decoding.
fn file_source_path(rest: &str) -> Option<&str> {
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') || path.len() < 2 {
        return None;
    }
    // `/C:/x` is a Windows drive path with a URL's leading slash.
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        return Some(&path[1..]);
    }
    Some(path)
}

/// `<arch>-<os>` for this build, the form the lock's `platform` takes: `x86_64-windows`, `aarch64-macos`, `x86_64-linux`.
pub fn current_platform() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// The tools directory from the environment, taking the lookup as a parameter so every branch is testable. The explicit
/// `CODERIPPER_TOOLS_DIR` wins; otherwise it is `tools` beside the build cache: the build cache is `<root>/build`, the tools are
/// `<root>/tools` (when `CODERIPPER_CACHE_DIR` names the build cache directly, the tools directory is its sibling `tools`).
pub fn tools_dir_from_env(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let present = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let under = |base: String| PathBuf::from(base).join("coderipper").join("tools");
    let dir = if let Some(dir) = present("CODERIPPER_TOOLS_DIR") {
        PathBuf::from(dir)
    } else if let Some(dir) = present("CODERIPPER_CACHE_DIR") {
        // A relative build-cache directory would put the tools beside the current directory: make it absolute first.
        let cache = std::path::absolute(dir).ok()?;
        cache.parent()?.join("tools")
    } else if let Some(base) = present("LOCALAPPDATA") {
        under(base)
    } else if let Some(base) = present("XDG_CACHE_HOME") {
        under(base)
    } else {
        PathBuf::from(present("HOME")?)
            .join(".cache")
            .join("coderipper")
            .join("tools")
    };
    // Tools are run by absolute path, so the directory must not depend on the working directory.
    std::path::absolute(dir).ok()
}

/// Whether the user has said a tool may be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// The user asked for installation (`--install-tools`).
    Granted,
    /// They have not. The default, in CI and everywhere else.
    NotGiven,
}

/// Where the bytes of a download come from. The CLI uses [`DefaultFetcher`]; tests supply their own.
pub trait Fetcher {
    /// Returns the whole file at `source`.
    fn fetch(&self, source: &str) -> Result<Vec<u8>, String>;
}

/// Reads `file://` sources from disk and `https://` (or loopback `http://`) sources over the network.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultFetcher;

/// A tool bigger than this is refused rather than held in memory.
const MAX_TOOL_BYTES: u64 = 512 * 1024 * 1024;

impl Fetcher for DefaultFetcher {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, String> {
        if let Some(path) = source.strip_prefix("file://") {
            let path = file_source_path(path)
                .ok_or_else(|| format!("{source} is not an absolute local file:// path"))?;
            return read_limited(std::fs::File::open(path).map_err(|e| e.to_string())?);
        }
        http_get(source)
    }
}

/// Reads at most [`MAX_TOOL_BYTES`] from `reader`; more is an error, not a truncated tool.
fn read_limited(reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    reader
        .take(MAX_TOOL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_TOOL_BYTES {
        return Err(format!("over the {MAX_TOOL_BYTES}-byte limit"));
    }
    Ok(bytes)
}

#[cfg(feature = "cli")]
fn http_get(source: &str) -> Result<Vec<u8>, String> {
    // An https source may redirect (release hosts do) but never down to http; a loopback http source never redirects.
    let secure = source.starts_with("https://");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(1800)))
        .https_only(secure)
        .max_redirects(if secure { 10 } else { 0 })
        .build()
        .into();
    let mut response = agent.get(source).call().map_err(|e| e.to_string())?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_TOOL_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

#[cfg(not(feature = "cli"))]
fn http_get(source: &str) -> Result<Vec<u8>, String> {
    Err(format!(
        "this build has no HTTP client (the `cli` feature is off), so {source} cannot be fetched; pass a Fetcher of your own"
    ))
}

/// Whether a tool is in the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// There, and its hash matches the lock.
    Installed,
    /// Not there.
    NotInstalled,
    /// There, but its hash no longer matches the lock.
    Corrupt,
}

/// The tools directory, and what can be done in it.
#[derive(Debug, Clone)]
pub struct ToolCache {
    root: PathBuf,
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write;
    digest.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The hash of a file, streamed (an installed tool can be large).
fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

impl ToolCache {
    /// A cache rooted at `root` (the tools directory itself). Nothing is created until a tool is installed.
    pub fn new(root: PathBuf) -> Self {
        ToolCache { root }
    }

    /// The tools directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The absolute path `entry` is (or would be) installed at: `<root>/<tool>/<version>/<file>`.
    pub fn path_of(&self, entry: &ToolEntry) -> PathBuf {
        let mut file = entry.file.clone().unwrap_or_else(|| entry.name.clone());
        if entry.platform.ends_with("windows") && !file.to_ascii_lowercase().ends_with(".exe") {
            file.push_str(".exe");
        }
        self.root.join(&entry.name).join(&entry.version).join(file)
    }

    /// Whether `entry` is installed. Reading the status never changes the cache.
    pub fn status(&self, entry: &ToolEntry) -> ToolStatus {
        match sha256_file(&self.path_of(entry)) {
            Err(_) => ToolStatus::NotInstalled,
            Ok(actual) if actual.eq_ignore_ascii_case(entry.installed_sha256()) => {
                ToolStatus::Installed
            }
            Ok(_) => ToolStatus::Corrupt,
        }
    }

    /// The absolute path of `name` on `platform`, installing it first when it is missing and `consent` allows it.
    ///
    /// An installed file is re-hashed first; one that no longer matches the lock is deleted and reported as `checksum_mismatch`
    /// (the next call reinstalls it). A download is hashed before anything is written, so a mismatch leaves no file behind.
    pub fn ensure(
        &self,
        lock: &ToolsLock,
        name: &str,
        platform: &str,
        consent: Consent,
        fetcher: &dyn Fetcher,
    ) -> Result<PathBuf, ToolError> {
        let Some(entry) = lock.for_platform(name, platform) else {
            return Err(if lock.knows(name) {
                ToolError::UnavailableForPlatform {
                    tool: name.to_string(),
                    platform: platform.to_string(),
                }
            } else {
                ToolError::Missing(name.to_string())
            });
        };
        let path = self.path_of(entry);
        let mismatch =
            |expected: &str, actual: String, note: &'static str| ToolError::ChecksumMismatch {
                tool: entry.name.clone(),
                version: entry.version.clone(),
                expected: expected.to_ascii_lowercase(),
                actual,
                note,
            };
        match sha256_file(&path) {
            Ok(actual) if actual.eq_ignore_ascii_case(entry.installed_sha256()) => return Ok(path),
            Ok(actual) => {
                // An installed file that no longer matches. Replacing it is a write: only with consent.
                if consent != Consent::Granted {
                    return Err(mismatch(
                        entry.installed_sha256(),
                        actual,
                        "the installed file was left as it is; run the install command with --install-tools to replace it",
                    ));
                }
                std::fs::remove_file(&path).map_err(|e| install_failed(entry, e.to_string()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(install_failed(entry, e.to_string())),
        }
        if consent != Consent::Granted {
            return Err(ToolError::ConsentNotGiven {
                tool: entry.name.clone(),
                version: entry.version.clone(),
            });
        }
        let bytes = fetcher
            .fetch(&entry.source)
            .map_err(|reason| ToolError::DownloadFailed {
                tool: entry.name.clone(),
                version: entry.version.clone(),
                source_url: entry.source.clone(),
                reason,
            })?;
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(mismatch(
                &entry.sha256,
                actual,
                "the download was discarded",
            ));
        }
        // An archive has passed its own checksum; now take the one member out and check that too, before anything is written.
        let bytes = match (entry.archive, &entry.member) {
            (Some(kind), Some(member)) => {
                let member_bytes = archive::extract_member(kind, &bytes, member, MAX_TOOL_BYTES)
                    .map_err(|e| match e {
                        archive::ArchiveError::Refused(reason) => ToolError::ArchiveRefused {
                            tool: entry.name.clone(),
                            version: entry.version.clone(),
                            reason,
                        },
                        archive::ArchiveError::Failed(reason) => install_failed(entry, reason),
                    })?;
                let actual = sha256_hex(&member_bytes);
                if !actual.eq_ignore_ascii_case(entry.installed_sha256()) {
                    return Err(mismatch(
                        entry.installed_sha256(),
                        actual,
                        "the archive's member did not match file_sha256 and was discarded",
                    ));
                }
                member_bytes
            }
            _ => bytes,
        };
        write_atomically(&path, &bytes).map_err(|e| install_failed(entry, e.to_string()))?;
        Ok(path)
    }
}

fn install_failed(entry: &ToolEntry, reason: String) -> ToolError {
    ToolError::InstallFailed {
        tool: entry.name.clone(),
        version: entry.version.clone(),
        reason,
    }
}

/// Writes `bytes` to a uniquely named sibling temporary file, syncs it and renames it into place, so a reader never sees half
/// a tool and two installers never share a temporary file.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().expect("a tool path has a directory");
    std::fs::create_dir_all(dir)?;
    let mut partial = tempfile::Builder::new()
        .prefix(".partial-")
        .tempfile_in(dir)?;
    partial.write_all(bytes)?;
    partial.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        partial
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    partial.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod tests;
