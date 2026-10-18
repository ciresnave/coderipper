//! The rules a tool lock's entries must satisfy, as plain functions over strings.
//!
//! This file has no dependency on the rest of the crate on purpose: `build.rs` includes it with `#[path]` and runs [`check`]
//! over the shipped `tools.lock`, so a lock with a bad hash or an unsafe name fails the BUILD, and the run-time parser
//! (`ToolsLock::parse`) applies exactly the same rules to any other lock.

#![allow(
    dead_code,
    reason = "build.rs includes this file and uses only some of the functions"
)]

/// One lock entry's fields, borrowed.
pub struct Fields<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub platform: &'a str,
    pub source: &'a str,
    pub sha256: &'a str,
    pub licence: &'a str,
    pub file: Option<&'a str>,
    pub archive: Option<&'a str>,
    pub member: Option<&'a str>,
    pub file_sha256: Option<&'a str>,
}

/// Why `fields` is not a valid entry, or `Ok`.
pub fn check(f: &Fields) -> Result<(), String> {
    for (label, value) in [("name", f.name), ("version", f.version)] {
        if !is_safe_component(value) {
            return Err(format!(
                "the {label} {value:?} is not a safe path component"
            ));
        }
    }
    if let Some(file) = f.file {
        if !is_safe_component(file) {
            return Err(format!(
                "the file name {file:?} is not a safe path component"
            ));
        }
    }
    if f.platform.is_empty()
        || !f
            .platform
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!("the platform {:?} is not <arch>-<os>", f.platform));
    }
    if !is_sha256(f.sha256) {
        return Err("sha256 must be 64 hex digits".to_string());
    }
    match (f.archive, f.member, f.file_sha256) {
        (None, None, None) => {}
        (Some(_), Some(member), Some(hash)) => {
            if member.is_empty() || !member.split('/').all(is_safe_component) {
                return Err(format!(
                    "the member {member:?} is not a plain relative path of safe components"
                ));
            }
            if !is_sha256(hash) {
                return Err("file_sha256 must be 64 hex digits".to_string());
            }
        }
        _ => {
            return Err(
                "archive, member and file_sha256 go together: all three or none".to_string(),
            )
        }
    }
    if f.licence.trim().is_empty() {
        return Err("a licence (SPDX expression) is required".to_string());
    }
    if !is_allowed_source(f.source) {
        return Err(format!(
            "the source {:?} must be https://, http:// on this machine, or file://",
            f.source
        ));
    }
    Ok(())
}

pub fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// A name that is one plain path component on every platform CodeRipper runs on.
pub fn is_safe_component(name: &str) -> bool {
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

pub fn is_allowed_source(source: &str) -> bool {
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
pub fn file_source_path(rest: &str) -> Option<&str> {
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
