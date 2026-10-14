//! Removes *known* secret shapes from third-party text before it is shown to someone else.
//!
//! CodeRipper runs other people's build scripts and tools and prints some of what they wrote. [`text`] is the one function that
//! takes such a string and replaces what it recognises as a credential with [`REDACTED`], returning how many replacements it made.
//!
//! # What it is, and is not
//!
//! - **It removes known keys and shapes, nothing more.** A base64 string with no key beside it is invisible to it. The claim to
//!   make about its output is "known shapes and keys were removed", never "the output is clean". Completeness belongs to the
//!   delegated secret scanner (the `SEC` rules), not to this module.
//! - **It never drops a message.** It replaces values inside the text, so a failure's existence, and every word that is not a
//!   recognised value, survive; it neither reads nor changes an exit code. A replaced value can be shorter *or* longer than
//!   `<redacted>` (`pwd=1` grows), so do not rely on the output being shorter.
//! - **It is idempotent**: redacting redacted text changes nothing and counts nothing.
//! - **It over-redacts on purpose** where a key looks like a secret's name (`tokenizer=bpe` loses `bpe`; `ssh://git@host` loses
//!   `git`; a value that follows `Authorization:` and is not a known scheme loses two words). A false positive hides a word; a
//!   false negative publishes a secret. It does try not to hide a developer's own diagnostics: `crate::auth::Claims`,
//!   `src/session.rs:12:5` and rustc's ``expected token: `;` `` are left alone.
//! - **The count is an upper bound on values:** one value that two rules both recognise (`password=https://u:p@h`) counts twice.
//! - **It removes ANSI escape sequences and invisible characters itself** (zero-width spaces and joiners, soft hyphens, directional
//!   marks, the byte-order mark), so a secret split by one cannot hide; other control characters are the caller's to clean.
//! - **Order matters to its caller.** Apply it *before* any length cut (so a cut cannot leave half a secret).
//!
//! Known gaps, stated so nobody relies on more than is here: a multi-parameter `Digest` header keeps its later parameters; a
//! short word after `Bearer`/`Basic` (under 16 characters, or under 32 with no digit) is taken to be English; a password in a
//! URL that starts with a digit and contains `/`, `?` or `#` reads as a port; `--password hunter2` (a flag with a space) is
//! not seen unless the word is long and high-entropy. It does not remove file-system paths that contain a user name, and it does
//! not make text safe to render as HTML.

/// What every removed value is replaced by.
pub const REDACTED: &str = "<redacted>";

/// Whether third-party text is redacted on its way out of a run.
///
/// Third-party text is what a build script, a tool or the compiler wrote about the project being checked: a build failure's
/// `Cause:`, an external module's stderr, a delegated tool's stderr, a rule's notes. On a developer's own machine that text is
/// the developer's, so the default is [`RedactionPolicy::Off`]; a service that shows it to someone else sets
/// [`RedactionPolicy::On`] with [`set_policy`] (the CLI's `--redact` does).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum RedactionPolicy {
    /// Third-party text is shown as it came. The default.
    #[default]
    Off,
    /// Known secret shapes are removed from third-party text ([`text`]); the output of a build script that failed and the
    /// stderr of an external module are withheld outright and replaced by a sentence built from facts CodeRipper knows (the
    /// package, its version and the exit status). Redaction never changes an exit code, and a run that failed still says so.
    On,
}

/// The process-wide policy: 0 is [`RedactionPolicy::Off`], 1 is [`RedactionPolicy::On`].
static POLICY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

#[cfg(test)]
thread_local! {
    /// A per-thread override of the policy, for in-crate tests (see [`with_policy`]).
    static OVERRIDE: std::cell::Cell<Option<RedactionPolicy>> = const { std::cell::Cell::new(None) };
}

/// Sets the policy for this process. Call it once at start-up, before any run: it applies to every run that follows, on every
/// thread. The library's own run functions ([`crate::run_checks_in_with_tools`], [`crate::run_workspace_in_with_tools`],
/// [`crate::run_checks_with`], [`crate::run_module`]) apply it to the whole [`crate::RunResult`] they return; a result you
/// build or deserialise yourself is covered by [`crate::RunResult::redact`].
pub fn set_policy(policy: RedactionPolicy) {
    let value = u8::from(policy == RedactionPolicy::On);
    POLICY.store(value, std::sync::atomic::Ordering::SeqCst);
}

/// The policy in force: [`RedactionPolicy::Off`] until [`set_policy`] says otherwise.
#[must_use]
pub fn policy() -> RedactionPolicy {
    if let Some(over) = test_override() {
        return over;
    }
    match POLICY.load(std::sync::atomic::Ordering::SeqCst) {
        0 => RedactionPolicy::Off,
        _ => RedactionPolicy::On,
    }
}

#[cfg(test)]
fn test_override() -> Option<RedactionPolicy> {
    OVERRIDE.with(std::cell::Cell::get)
}

#[cfg(not(test))]
fn test_override() -> Option<RedactionPolicy> {
    None
}

/// Runs `f` with `policy` in force **on this thread only**, restoring the previous override afterwards (also when `f`
/// panics). For in-crate tests, which run in parallel threads and so cannot use the process-wide setting.
#[cfg(test)]
pub(crate) fn with_policy<T>(policy: RedactionPolicy, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<RedactionPolicy>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(OVERRIDE.with(|o| o.replace(Some(policy))));
    f()
}

/// Whether the policy is [`RedactionPolicy::On`].
pub(crate) fn is_on() -> bool {
    policy() == RedactionPolicy::On
}

/// `s` as it may be shown under the policy in force: unchanged when it is off, otherwise [`text`] of it. Call it on text that
/// has been cleaned of control characters and *before* a length cut, so a cut cannot leave half a secret.
pub(crate) fn scrub(s: &str) -> String {
    if !is_on() {
        return s.to_string();
    }
    text(s).0
}

/// The start of the note that says how many values a result had redacted; an existing one is replaced, never repeated.
const NOTE_MARKER: &str = "value(s) were redacted from third-party output";

impl crate::RunResult {
    /// Replaces known secret shapes (see [`text`]) in every string of this result that third-party output can reach: `errors`,
    /// `notes`, each finding's `summary`, `detail`, `subject`, `member`, `project` and `positive_control` and its location's
    /// file, and each waiver's `reason`. Returns how many values this call replaced.
    ///
    /// When the result holds any [`REDACTED`] placeholder afterwards (this call's, or the run's own sinks' under
    /// [`RedactionPolicy::On`]), one note says how many, counted from the result itself, so it is right for each result however
    /// many runs share the process. **No note does not mean nothing was redacted from the run's raw output**: a run whose
    /// policy was off was never redacted. It changes neither the number of findings nor the errors' existence, so the exit code a
    /// caller derives from the result is the same. Idempotent: a second call replaces nothing and leaves the result as it was.
    ///
    /// The library's run functions call it themselves under [`RedactionPolicy::On`]; call it on a result you deserialised or
    /// assembled yourself before you show it.
    pub fn redact(&mut self) -> usize {
        self.notes.retain(|n| !n.contains(NOTE_MARKER));
        let mut replaced = 0;
        let mut placeholders = 0;
        let mut fix = |s: &mut String| {
            let (out, n) = text(s);
            if n > 0 {
                *s = out;
                replaced += n;
            }
            placeholders += s.matches(REDACTED).count();
        };
        self.errors.iter_mut().for_each(&mut fix);
        self.notes.iter_mut().for_each(&mut fix);
        for waived in &mut self.waived {
            fix(&mut waived.reason);
            // the wide file's path (a home directory has the user's name in it)
            if let Some(source) = &mut waived.source {
                fix(source);
            }
        }
        let findings = self
            .findings
            .iter_mut()
            .chain(self.waived.iter_mut().map(|w| &mut w.finding));
        for finding in findings {
            fix(&mut finding.summary);
            fix(&mut finding.detail);
            fix(&mut finding.project);
            if let Some(location) = &mut finding.location {
                fix(&mut location.file);
            }
            let optional = [
                &mut finding.subject,
                &mut finding.member,
                &mut finding.positive_control,
            ];
            for value in optional.into_iter().flatten() {
                fix(value);
            }
        }
        if placeholders > 0 {
            self.notes.push(format!(
                "{placeholders} {NOTE_MARKER} (known secret shapes and keys only; this does not mean no secret remains)"
            ));
        }
        replaced
    }
}

/// Applies [`crate::RunResult::redact`] when the policy is on. What the library's public run functions end with.
pub(crate) fn finish(result: &mut crate::RunResult) {
    if is_on() {
        result.redact();
    }
}

/// `input` with every recognised credential replaced by [`REDACTED`], and how many replacements were made.
///
/// See the [module documentation](self) for what is recognised and what is not. Text that is already redacted counts nothing.
#[must_use]
pub fn text(input: &str) -> (String, usize) {
    let mut count = 0;
    let cleaned = without_invisible(input);
    let s = private_key_blocks(&cleaned, &mut count);
    let s = url_userinfo(&s, &mut count);
    let s = authorization_headers(&s, &mut count);
    let s = keyed_values(&s, &mut count);
    let s = spaced_entropy_values(&s, &mut count);
    let s = token_shapes(&s, &mut count);
    (s, count)
}

/// `input` without ANSI escape sequences and without the characters that render as nothing (soft hyphen, zero-width space and
/// joiners, directional marks, word joiner, byte-order mark): a secret split by one of them reads as whole to a person and as
/// two halves to a pattern.
fn without_invisible(input: &str) -> String {
    let invisible = |c: char| {
        matches!(
            c,
            '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}'
        )
    };
    if !input.contains(['\u{1b}', '\u{00AD}', '\u{2060}', '\u{FEFF}'])
        && !input.chars().any(invisible)
    {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // parameters and intermediates, then one final byte in `@`..=`~`
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
        } else if !invisible(c) {
            out.push(c);
        }
    }
    out
}

/// Names that, as a substring of a key with its separators removed and case folded, mark the key's value as a secret.
const SENSITIVE_SUBSTRINGS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "pwd",
    "apikey",
    "credential",
    "privatekey",
    "session",
    "cookie",
];

/// Names that mark a key as sensitive only when they are a *whole* segment of it (`auth_token`, `x-auth`): as a substring they
/// would match `author` and `authority`.
const SENSITIVE_SEGMENTS: &[&str] = &["auth", "oauth", "authkey", "authorization"];

/// A well-known token shape: a fixed prefix followed by a run of an alphabet. The list is short on purpose; it is not a scanner.
struct Shape {
    /// What the token starts with (case-sensitive).
    prefix: &'static str,
    /// The fewest characters of the alphabet that must follow the prefix.
    min_tail: usize,
    /// The most that may follow when the shape has a fixed length; the character after it must then not be alphanumeric.
    max_tail: Option<usize>,
    /// Whether `c` can be part of the token after the prefix.
    alphabet: fn(char) -> bool,
}

fn alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}
fn alnum_underscore(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
fn alnum_dash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}
fn upper_digit(c: char) -> bool {
    c.is_ascii_uppercase() || c.is_ascii_digit()
}

const fn shape(
    prefix: &'static str,
    min_tail: usize,
    max_tail: Option<usize>,
    alphabet: fn(char) -> bool,
) -> Shape {
    Shape {
        prefix,
        min_tail,
        max_tail,
        alphabet,
    }
}

/// The shapes, as the issuers describe them publicly (shapes only, none of them checked against a live issuer here).
const SHAPES: &[Shape] = &[
    // GitHub personal, OAuth, user-to-server, server-to-server and refresh tokens.
    shape("ghp_", 20, None, alnum),
    shape("gho_", 20, None, alnum),
    shape("ghu_", 20, None, alnum),
    shape("ghs_", 20, None, alnum),
    shape("ghr_", 20, None, alnum),
    // GitHub fine-grained personal access token.
    shape("github_pat_", 20, None, alnum_underscore),
    // AWS access key id: `AKIA` (long-term) or `ASIA` (temporary) and 16 upper-case letters or digits.
    shape("AKIA", 16, Some(16), upper_digit),
    shape("ASIA", 16, Some(16), upper_digit),
    // npm access token: `npm_` and about 36 alphanumerics.
    shape("npm_", 30, None, alnum),
    // crates.io API token: `cio` and about 32 alphanumerics.
    shape("cio", 28, None, alnum),
    // Slack tokens: `xoxa-`, `xoxb-`, `xoxp-`, `xoxr-`, `xoxs-`.
    shape("xoxa-", 10, None, alnum_dash),
    shape("xoxb-", 10, None, alnum_dash),
    shape("xoxp-", 10, None, alnum_dash),
    shape("xoxr-", 10, None, alnum_dash),
    shape("xoxs-", 10, None, alnum_dash),
];

/// Authentication schemes that follow `Authorization:` and are not themselves the secret.
const AUTH_SCHEMES: &[&str] = &[
    "bearer",
    "basic",
    "token",
    "digest",
    "negotiate",
    "ntlm",
    "apikey",
];

/// The ends of a private-key marker line (`-----BEGIN RSA PRIVATE KEY-----`, `-----END PGP PRIVATE KEY BLOCK-----`).
const KEY_MARKER_ENDS: &[&str] = &["PRIVATE KEY BLOCK-----", "PRIVATE KEY-----"];

/// A private-key block: `-----BEGIN ... PRIVATE KEY-----` through the matching `-----END ...`, to the end of the text when the
/// block is cut off there, and from the start of the text when only the `-----END ...` line is present (a 4096-byte stderr
/// *tail* can begin in the middle of a key).
fn private_key_blocks(input: &str, count: &mut usize) -> String {
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "-----END ";
    if !input.contains(BEGIN) && !input.contains(END) {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut seen_begin = false;
    while i < chars.len() {
        if starts_with(&chars, i, BEGIN) {
            if let Some(body_from) = marker_end(&chars, i + BEGIN.len()) {
                seen_begin = true;
                out.push_str(REDACTED);
                *count += 1;
                i = find_end_marker(&chars, body_from);
                continue;
            }
        } else if starts_with(&chars, i, END) && !seen_begin {
            if let Some(end) = marker_end(&chars, i + END.len()) {
                // The key's start was cut off: what is already in `out` is the key's body.
                out.clear();
                out.push_str(REDACTED);
                *count += 1;
                seen_begin = true;
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The index just past the `PRIVATE KEY-----` that ends a marker whose label starts at `from` (capitals and spaces, short), if
/// the marker is one.
fn marker_end(chars: &[char], from: usize) -> Option<usize> {
    let window_end = (from + 48).min(chars.len());
    let mut j = from;
    while j < window_end {
        for end in KEY_MARKER_ENDS {
            if starts_with(chars, j, end) {
                return Some(j + end.chars().count());
            }
        }
        if !(chars[j].is_ascii_uppercase() || chars[j] == ' ') {
            return None;
        }
        j += 1;
    }
    None
}

/// The index just past the `-----END ... PRIVATE KEY-----` line that closes a block whose body starts at `from`, or the end of
/// the text.
fn find_end_marker(chars: &[char], from: usize) -> usize {
    const END: &str = "-----END ";
    let mut i = from;
    while i < chars.len() {
        if starts_with(chars, i, END) {
            if let Some(end) = marker_end(chars, i + END.len()) {
                return end;
            }
        }
        i += 1;
    }
    chars.len()
}

/// `scheme://user:password@host` and `scheme://token@host` become `scheme://<redacted>@host`. A password may itself contain
/// `/`, `?` or `#`: when the authority has no `@` the token is read to its end, and an `@` there counts if a `user:` that is not a
/// `host:port` precedes it.
fn url_userinfo(input: &str, count: &mut usize) -> String {
    if !input.contains("://") {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if starts_with(&chars, i, "://") && i > 0 && chars[i - 1].is_ascii_alphanumeric() {
            let from = i + 3;
            let mut end = from;
            while end < chars.len() && !is_authority_end(chars[end]) {
                end += 1;
            }
            out.push_str("://");
            match last_at(&chars, from, end).or_else(|| password_with_delimiters(&chars, from)) {
                Some(a) if a > from => {
                    out.push_str(REDACTED);
                    *count += 1;
                    i = a;
                }
                _ => i = from,
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn last_at(chars: &[char], from: usize, end: usize) -> Option<usize> {
    (from..end).rev().find(|&k| chars[k] == '@')
}

/// The index of the `@` of a `user:password@host` whose password holds `/`, `?` or `#`, read from `from` to the end of the
/// whitespace-free token; `None` when the token has no such shape (`host:8080/path@x` is a port and a path, not a login).
fn password_with_delimiters(chars: &[char], from: usize) -> Option<usize> {
    let mut end = from;
    while end < chars.len() && !is_token_end(chars[end]) {
        end += 1;
    }
    let at = last_at(chars, from, end)?;
    let colon = (from..at).find(|&k| chars[k] == ':')?;
    let slash = (from..at).find(|&k| matches!(chars[k], '/' | '?' | '#'));
    let before_colon_ok = slash.is_none_or(|s| s > colon);
    let port_like = chars.get(colon + 1).is_some_and(char::is_ascii_digit);
    (before_colon_ok && !port_like && at > colon + 1).then_some(at)
}

fn is_authority_end(c: char) -> bool {
    is_token_end(c) || matches!(c, '/' | '?' | '#')
}

fn is_token_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`')
}

/// `Authorization: <scheme> <credential>` keeps the scheme word and replaces the credential (the header may be quoted, as in
/// JSON or a Python `repr`); a value whose first word is not a known scheme loses it and the word after it; a standalone
/// `Bearer <token>` or `Basic <token>` replaces the token when it looks like one (16 or more characters with a digit, or 32+).
fn authorization_headers(input: &str, count: &mut usize) -> String {
    let lower = input.to_ascii_lowercase();
    if !(lower.contains("authorization") || lower.contains("bearer") || lower.contains("basic")) {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    // ASCII lowering changes bytes one for one and no char count, so an index into `low` is an index into `chars`.
    let low: Vec<char> = lower.chars().collect();
    debug_assert_eq!(chars.len(), low.len());
    let mut replace: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_word_start = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        if at_word_start && starts_with(&low, i, "authorization") {
            if let Some(end) =
                authorization_value(&chars, &low, i + "authorization".len(), &mut replace)
            {
                i = end;
                continue;
            }
        }
        if at_word_start && (starts_with(&low, i, "bearer ") || starts_with(&low, i, "basic ")) {
            let mut k = i;
            while k < chars.len() && chars[k].is_ascii_alphabetic() {
                k += 1;
            }
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
            let (t_from, t_end) = token_run(&chars, k);
            if looks_like_credential(&chars[t_from..t_end]) && !is_redacted(&chars[t_from..t_end]) {
                replace.push((t_from, t_end));
                i = t_end;
                continue;
            }
        }
        i += 1;
    }
    apply_replacements(&chars, &replace, count)
}

/// Reads what follows the word `authorization` (at `from`): an optional closing quote and backslashes, `:` or `=`, an optional
/// opening quote, then the value. Pushes the ranges to replace and returns where reading stopped.
fn authorization_value(
    chars: &[char],
    low: &[char],
    from: usize,
    replace: &mut Vec<(usize, usize)>,
) -> Option<usize> {
    let mut j = from;
    j = skip_while(chars, j, |c| matches!(c, '"' | '\'' | '\\'));
    j = skip_while(chars, j, |c| c == ' ');
    if !matches!(chars.get(j), Some(':' | '=')) {
        return None;
    }
    j += 1;
    j = skip_while(chars, j, |c| matches!(c, ' ' | '\t' | '"' | '\'' | '\\'));
    let (w_from, w_end) = word_at(chars, j);
    if w_end == w_from || is_redacted(&chars[w_from..w_end]) {
        return None;
    }
    let word: String = low[w_from..w_end].iter().collect();
    let mut stop = w_end;
    let credential_from = if AUTH_SCHEMES.contains(&word.as_str()) {
        skip_while(chars, w_end, |c| c == ' ')
    } else {
        // An unlisted scheme (`JWT`, `Bot`, `AWS`) is followed by its credential: take the first word and the second.
        replace.push((w_from, w_end));
        skip_while(chars, w_end, |c| c == ' ')
    };
    let (c_from, c_end) = word_at(chars, credential_from);
    if c_end > c_from && !is_redacted(&chars[c_from..c_end]) {
        replace.push((c_from, c_end));
        stop = c_end;
    }
    Some(stop)
}

fn skip_while(chars: &[char], mut at: usize, keep: impl Fn(char) -> bool) -> usize {
    while at < chars.len() && keep(chars[at]) {
        at += 1;
    }
    at
}

/// A value after a sensitive key: `KEY=value`, `KEY: value`, `"key": "value"` (also with escaped quotes), with a quoted value
/// running to its closing quote.
fn keyed_values(input: &str, count: &mut usize) -> String {
    if !input.contains(['=', ':']) {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let mut replace: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '=' | ':') {
            if let Some(end) = keyed_value_at(&chars, i, &mut replace) {
                i = end.max(i + 1);
                continue;
            }
        }
        i += 1;
    }
    apply_replacements(&chars, &replace, count)
}

/// If the separator at `sep` follows a sensitive key and is followed by a value, pushes the value's range and returns its end.
fn keyed_value_at(chars: &[char], sep: usize, replace: &mut Vec<(usize, usize)>) -> Option<usize> {
    let colon = chars[sep] == ':';
    // `crate::auth::Claims`: a path separator, not a key and its value.
    if colon && (chars.get(sep + 1) == Some(&':') || (sep > 0 && chars[sep - 1] == ':')) {
        return None;
    }
    let mut key_end = sep;
    while key_end > 0 && chars[key_end - 1] == ' ' {
        key_end -= 1;
    }
    while key_end > 0 && matches!(chars[key_end - 1], '"' | '\'' | '\\') {
        key_end -= 1;
    }
    let mut key_from = key_end;
    while key_from > 0 && is_key_char(chars[key_from - 1]) {
        key_from -= 1;
    }
    let key: String = chars[key_from..key_end].iter().collect();
    if key.is_empty() || !is_sensitive_key(&key) {
        return None;
    }
    let v = skip_while(chars, sep + 1, |c| matches!(c, ' ' | '\t'));
    // Skip backslashes of an escaped quote (`{\"password\":\"x\"}`) but keep them in the replaced range's left context.
    let value_from = skip_while(chars, v, |c| c == '\\');
    if value_from >= chars.len() {
        return None;
    }
    let (from, end) = if matches!(chars[value_from], '"' | '\'') {
        let quote = chars[value_from];
        let close = (value_from + 1..chars.len()).find(|&k| chars[k] == quote);
        (v, close.map_or(chars.len(), |c| c + 1))
    } else {
        let mut e = value_from;
        while e < chars.len() && !chars[e].is_whitespace() {
            e += 1;
        }
        // Trailing punctuation belongs to the sentence, not to the value.
        while e > value_from && matches!(chars[e - 1], ',' | ';' | ')' | ']' | '}' | '.') {
            e -= 1;
        }
        (value_from, e)
    };
    let value: String = chars[from..end].iter().collect();
    let bare = value.trim_matches(['"', '\'', '\\']);
    if bare.is_empty() || bare == REDACTED {
        return None;
    }
    if colon && is_diagnostic_value(&key, bare) {
        return None;
    }
    replace.push((from, end));
    Some(end)
}

/// A `key: value` that a compiler or a path produces rather than a login: a line and column after a file name
/// (`src/auth.rs:12:5`), a code-quoted token (``expected token: `;` ``), or a single character (`unknown start of token: “`).
fn is_diagnostic_value(key: &str, value: &str) -> bool {
    let line_col = value
        .split(':')
        .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    (key.contains('.') && line_col)
        || value.starts_with('`')
        || value.starts_with("\\u{")
        || value.chars().count() <= 1
}

/// A sensitive key word followed by a long, high-entropy word with no separator between them (`TOKEN 3f9a...`): replaced only
/// because of the key beside it, never on its own (a hash in a lockfile is not a secret).
fn spaced_entropy_values(input: &str, count: &mut usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let from = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        spans.push((from, i));
    }
    let mut replace: Vec<(usize, usize)> = Vec::new();
    for pair in spans.windows(2) {
        let (a_from, a_end) = pair[0];
        let (b_from, b_end) = pair[1];
        let key: String = chars[a_from..a_end]
            .iter()
            .collect::<String>()
            .trim_matches(['"', '\'', ':', '=', ','])
            .to_string();
        if key.is_empty() || !key.chars().all(is_key_char) || !is_sensitive_key(&key) {
            continue;
        }
        let word = &chars[b_from..b_end];
        let has_digit = word.iter().any(char::is_ascii_digit);
        let mixed_case =
            word.iter().any(char::is_ascii_uppercase) && word.iter().any(char::is_ascii_lowercase);
        if word.len() >= 32
            && word.iter().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-' | '.')
            })
            && (has_digit || mixed_case)
        {
            replace.push((b_from, b_end));
        }
    }
    apply_replacements(&chars, &replace, count)
}

/// The short list of [`SHAPES`], matched where a word starts.
fn token_shapes(input: &str, count: &mut usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut replace: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let at_start = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        if at_start {
            if let Some(end) = shape_end(&chars, i) {
                replace.push((i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    apply_replacements(&chars, &replace, count)
}

/// Where a token of a known shape that starts at `i` ends, if one does.
fn shape_end(chars: &[char], i: usize) -> Option<usize> {
    SHAPES.iter().find_map(|shape| {
        if !starts_with(chars, i, shape.prefix) {
            return None;
        }
        let from = i + shape.prefix.chars().count();
        let mut end = from;
        while end < chars.len() && (shape.alphabet)(chars[end]) {
            end += 1;
            if shape.max_tail.is_some_and(|m| end - from > m) {
                return None;
            }
        }
        // A fixed-length shape must end where the word does (`AKIA` + 16 capitals + lower-case letters is not a key id).
        if shape.max_tail.is_some() && chars.get(end).is_some_and(char::is_ascii_alphanumeric) {
            return None;
        }
        (end - from >= shape.min_tail).then_some(end)
    })
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

/// Whether `key` names a secret (see [`SENSITIVE_SUBSTRINGS`] and [`SENSITIVE_SEGMENTS`]).
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    // The header pass owns `Authorization`; handling it here too would redact the scheme word and leave the credential.
    if lower.ends_with("authorization") {
        return false;
    }
    let compact: String = lower.chars().filter(char::is_ascii_alphanumeric).collect();
    if SENSITIVE_SUBSTRINGS.iter().any(|s| compact.contains(s)) {
        return true;
    }
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|segment| SENSITIVE_SEGMENTS.contains(&segment))
}

/// The run of non-whitespace, non-quote characters at `from`.
fn word_at(chars: &[char], from: usize) -> (usize, usize) {
    let end = skip_while(chars, from, |c| {
        !c.is_whitespace() && !matches!(c, '"' | '\'' | ',' | ';' | '\\')
    });
    (from, end)
}

/// The run of characters at `from` that a bearer or basic credential is made of.
fn token_run(chars: &[char], from: usize) -> (usize, usize) {
    let end = skip_while(chars, from, |c| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '~' | '+' | '/' | '=' | '-')
    });
    (from, end)
}

/// A bare word after `Bearer` or `Basic` is a credential only when it looks like one, so "basic implementation" survives.
fn looks_like_credential(word: &[char]) -> bool {
    word.len() >= 32 || (word.len() >= 16 && word.iter().any(char::is_ascii_digit))
}

fn is_redacted(word: &[char]) -> bool {
    word.iter().copied().eq(REDACTED.chars())
}

fn starts_with(chars: &[char], at: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(k, n)| chars.get(at + k) == Some(&n))
}

/// `chars` with each `(from, end)` range (ascending, non-overlapping) replaced by [`REDACTED`]; counts one per range.
fn apply_replacements(chars: &[char], ranges: &[(usize, usize)], count: &mut usize) -> String {
    if ranges.is_empty() {
        return chars.iter().collect();
    }
    let mut out = String::with_capacity(chars.len());
    let mut at = 0;
    for &(from, end) in ranges {
        if from < at || end <= from {
            continue;
        }
        out.extend(&chars[at..from]);
        out.push_str(REDACTED);
        *count += 1;
        at = end;
    }
    out.extend(&chars[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture secrets are assembled at run time: this repository's own CI runs a secret scanner over it, and a literal fake
    /// token would trip it.
    fn join(parts: &[&str]) -> String {
        parts.concat()
    }

    fn tail(n: usize) -> String {
        "aB3dE5fG7h".chars().cycle().take(n).collect()
    }

    fn unchanged(s: &str) {
        assert_eq!(text(s), (s.to_string(), 0), "input: {s}");
    }

    fn becomes(input: &str, expect: &str, n: usize) {
        let (out, got) = text(input);
        assert_eq!(out, expect, "input: {input}");
        assert_eq!(got, n, "input: {input}");
    }

    #[test]
    fn url_userinfo_with_a_password_is_removed() {
        let url = join(&[
            "https://",
            "alice",
            ":",
            "hunter2hunter2",
            "@",
            "registry.example/path",
        ]);
        becomes(
            &format!("fetching {url} failed"),
            "fetching https://<redacted>@registry.example/path failed",
            1,
        );
    }

    #[test]
    fn url_userinfo_with_only_a_token_is_removed() {
        let url = join(&["https://", &tail(24), "@", "host.example"]);
        becomes(&url, "https://<redacted>@host.example", 1);
    }

    #[test]
    fn a_password_holding_url_delimiters_or_a_digit_scheme_is_removed() {
        becomes(
            &join(&[
                "postgres://",
                "admin",
                ":",
                "p#ss/w?rd",
                "@",
                "db.example/x",
            ]),
            "postgres://<redacted>@db.example/x",
            1,
        );
        becomes(
            &join(&["s3://", "key", ":", "sec", "@", "bucket"]),
            "s3://<redacted>@bucket",
            1,
        );
    }

    #[test]
    fn urls_without_userinfo_survive_even_with_an_at_sign_later() {
        unchanged("see https://example.org/a@b and https://example.org/");
        unchanged("https://crates.io:443/api/v1/crates/foo@1.0/download");
        unchanged("https://example.org/p?mail=a@b.example");
    }

    #[test]
    fn keyed_values_are_removed_in_each_separator_style() {
        let v = tail(20);
        for (input, expect) in [
            (
                format!("GITHUB_TOKEN={v} ran"),
                "GITHUB_TOKEN=<redacted> ran",
            ),
            (format!("password: {v}"), "password: <redacted>"),
            (format!("password:\t{v}"), "password:\t<redacted>"),
            (
                format!("api_key = \"{v} with space\" end"),
                "api_key = <redacted> end",
            ),
            (
                format!("{{\"secret\": \"{v}\"}}"),
                "{\"secret\": <redacted>}",
            ),
            (format!("{{\"secret\":\"{v}\"}}"), "{\"secret\":<redacted>}"),
            (format!("--auth-token={v}"), "--auth-token=<redacted>"),
            (
                format!("export SESSION_ID='{v}'"),
                "export SESSION_ID=<redacted>",
            ),
            (
                format!("{{\\\"password\\\":\\\"{v}\\\"}}"),
                "{\\\"password\\\":<redacted>}",
            ),
        ] {
            becomes(&input, expect, 1);
        }
    }

    #[test]
    fn a_value_that_starts_with_the_placeholder_is_still_redacted() {
        becomes(
            &format!("TOKEN=<redacted>{}", tail(12)),
            "TOKEN=<redacted>",
            1,
        );
        becomes(
            &format!("Authorization: Bearer <redacted>{}", tail(12)),
            "Authorization: Bearer <redacted>",
            1,
        );
    }

    #[test]
    fn lookalike_keys_and_values_survive() {
        for s in [
            "author=name",
            "authority: example.org",
            "checksum = \"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08\"",
            "error: could not find directory of OpenSSL installation",
            "version = \"1.2.3\"",
            "see the basic implementation notes",
            "",
            "\"",
            "=",
            ":",
        ] {
            unchanged(s);
        }
    }

    #[test]
    fn a_developers_diagnostics_are_not_eaten() {
        for s in [
            "use crate::auth::jwt::Claims;",
            "use syn::token::Semi;",
            "--> src/auth.rs:12:5",
            "at (src/session.rs:2)",
            "error: expected token: `;`",
            "error: unexpected token: `}`",
            "error: unknown start of token: \u{201c}",
            "error[E0425]: cannot find value `session` in this scope",
        ] {
            unchanged(s);
        }
    }

    #[test]
    fn authorization_headers_keep_the_scheme_and_lose_the_credential() {
        let cred = tail(24);
        for (input, expect) in [
            (
                format!("Authorization: Bearer {cred}"),
                "Authorization: Bearer <redacted>",
            ),
            (
                format!("authorization: Basic {cred}=="),
                "authorization: Basic <redacted>",
            ),
            (
                format!("Proxy-Authorization: {cred}"),
                "Proxy-Authorization: <redacted>",
            ),
            (
                format!("sent Bearer {cred} to host"),
                "sent Bearer <redacted> to host",
            ),
            (
                format!("{{\"Authorization\": \"Bearer {cred}\"}}"),
                "{\"Authorization\": \"Bearer <redacted>\"}",
            ),
            (
                format!("{{\"Authorization\":\"{cred}\"}}"),
                "{\"Authorization\":\"<redacted>\"}",
            ),
            (
                format!("'Authorization': 'Bearer {cred}'"),
                "'Authorization': 'Bearer <redacted>'",
            ),
            (
                format!("Authorization: JWT {cred}"),
                "Authorization: <redacted> <redacted>",
            ),
        ] {
            let (out, n) = text(&input);
            assert_eq!(out, expect, "input: {input}");
            assert!(n >= 1, "input: {input}");
            assert!(!out.contains(&cred), "credential survived: {out}");
        }
    }

    #[test]
    fn well_known_token_shapes_are_removed_without_a_key() {
        let shapes = [
            join(&["ghp_", &tail(36)]),
            join(&["gho_", &tail(36)]),
            join(&["github_pat_", &tail(40)]),
            join(&["AKIA", "ABCDEFGH12345678"]),
            join(&["ASIA", "ABCDEFGH12345678"]),
            join(&["npm_", &tail(36)]),
            join(&["cio", &tail(32)]),
            join(&["xoxb-", "1234567890-abcdefghij"]),
        ];
        for token in shapes {
            let (out, n) = text(&format!("note: saw {token} in output"));
            assert_eq!(
                out,
                "note: saw <redacted> in output",
                "token shape: {}",
                &token[..4]
            );
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn near_misses_of_token_shapes_survive() {
        for s in [
            join(&["ghp_", "short"]),
            join(&["AKIA", "ABCDEFGH123456789012"]),
            join(&["AKIA", "ABCDEFGH12345678", "abc"]),
            join(&["xoxb-", "tiny"]),
            join(&["precio", &tail(40)]),
        ] {
            unchanged(&s);
        }
    }

    #[test]
    fn a_private_key_block_is_removed_whole_cut_off_at_either_end_or_pgp() {
        let begin = join(&["-----BEGIN ", "RSA PRIVATE KEY", "-----"]);
        let end = join(&["-----END ", "RSA PRIVATE KEY", "-----"]);
        becomes(
            &format!("before {begin} MIIEowIBAAKCAQEA1234 {end} after"),
            "before <redacted> after",
            1,
        );
        becomes(
            &format!("before {begin} MIIEowIBAAKCAQEA1234"),
            "before <redacted>",
            1,
        );
        // The tail of a long stderr can start inside the key.
        becomes(&format!("QEA1234abcd {end} after"), "<redacted> after", 1);
        let pgp_begin = join(&["-----BEGIN ", "PGP PRIVATE KEY BLOCK", "-----"]);
        let pgp_end = join(&["-----END ", "PGP PRIVATE KEY BLOCK", "-----"]);
        becomes(
            &format!("x {pgp_begin} lQOY {pgp_end} y"),
            "x <redacted> y",
            1,
        );
        unchanged("-----BEGIN PUBLIC KEY----- MFkw -----END PUBLIC KEY-----");
    }

    #[test]
    fn a_long_word_after_a_key_with_no_separator_is_removed_but_not_on_its_own() {
        let blob = tail(40);
        becomes(&format!("TOKEN {blob} rest"), "TOKEN <redacted> rest", 1);
        let jwt = join(&["eyJhbGciOi", &tail(20), ".", &tail(24), ".", &tail(24)]);
        becomes(&format!("TOKEN {jwt} rest"), "TOKEN <redacted> rest", 1);
        unchanged(&format!("hash {blob} rest"));
    }

    #[test]
    fn redaction_is_idempotent_and_counts_each_value_once() {
        let url = join(&["https://", "u", ":", "p4ssw0rd", "@", "h.example"]);
        let input = format!(
            "{url} TOKEN={} Authorization: Bearer {} {}",
            tail(20),
            tail(30),
            join(&["ghp_", &tail(36)])
        );
        let (once, n) = text(&input);
        assert_eq!(n, 4, "output: {once}");
        let (twice, again) = text(&once);
        assert_eq!(once, twice);
        assert_eq!(again, 0);
    }

    #[test]
    fn a_secret_is_never_half_removed_when_the_text_is_cut_inside_it() {
        // The caller redacts before it cuts; redact-then-cut leaves no part of the token whatever the cut position.
        let token = join(&["ghp_", &tail(36)]);
        let whole = format!("{} {token} tail", "x".repeat(380));
        let (out, _) = text(&whole);
        for cut_at in 380..out.chars().count() {
            let cut: String = out.chars().take(cut_at).collect();
            assert!(!cut.contains("ghp_a"), "cut at {cut_at}: {cut}");
        }
    }

    #[test]
    fn redaction_keeps_the_message_and_its_surrounding_words() {
        let input = format!("build failed: TOKEN={} (exit status 101)", tail(20));
        becomes(
            &input,
            "build failed: TOKEN=<redacted> (exit status 101)",
            1,
        );
    }

    #[test]
    fn non_ascii_text_is_kept_intact() {
        becomes(
            "échec: «constructeur» ¿qué? 日本語 password: пароль123",
            "échec: «constructeur» ¿qué? 日本語 password: <redacted>",
            1,
        );
        let cred = tail(24);
        let (out, n) = text(&format!("é Authorization: Bearer {cred} ß"));
        assert_eq!(out, "é Authorization: Bearer <redacted> ß");
        assert_eq!(n, 1);
    }

    #[test]
    fn pathological_repeats_do_not_take_quadratic_time() {
        // 256 KiB of each shape; a pass that rescanned to the end of the text per occurrence would take minutes here.
        let started = std::time::Instant::now();
        for unit in [
            "token=<redacted>",
            "authorization:<redacted>",
            "token=\"",
            "://",
            "-----BEGIN ",
            "-----END ",
            "a@",
            "=",
            "Bearer ",
            "ghp_",
            "token ",
        ] {
            let input = unit.repeat(262_144 / unit.len());
            let (out, _) = text(&input);
            assert!(!out.is_empty());
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "took {:?}",
            started.elapsed()
        );
    }

    fn fake_finding(summary: &str, detail: &str) -> crate::finding::Finding {
        crate::finding::Finding::new(
            "rule-x",
            crate::finding::Severity::Low,
            crate::finding::Confidence::High,
            "proj",
            summary,
            detail,
        )
    }

    #[test]
    fn the_policy_is_off_until_it_is_set_and_scrub_follows_it() {
        let secret = join(&["TOKEN=", &tail(20)]);
        with_policy(RedactionPolicy::Off, || {
            assert_eq!(scrub(&secret), secret);
        });
        with_policy(RedactionPolicy::On, || {
            assert_eq!(scrub(&secret), "TOKEN=<redacted>");
        });
        assert_eq!(RedactionPolicy::default(), RedactionPolicy::Off);
    }

    #[test]
    fn a_result_is_redacted_in_every_field_third_party_text_can_reach() {
        let secret = join(&["ghp_", &tail(36)]);
        let keyed = join(&["password=", &tail(20)]);
        let mut finding = fake_finding(&format!("saw {secret}"), &format!("detail {keyed}"));
        finding.subject = Some(format!("subject {secret}"));
        finding.member = Some(format!("member {keyed}"));
        finding.positive_control = Some(format!("control {secret}"));
        let waived = crate::finding::Waived {
            finding: fake_finding(&format!("waived {secret}"), "plain"),
            reason: format!("reason {keyed}"),
            source: None,
            declared_public: false,
        };
        let mut result = crate::RunResult::new(
            vec![finding],
            vec![format!("error {secret}"), "plain error".to_string()],
        );
        result.notes.push(format!("note {keyed}"));
        result.waived.push(waived);

        let n = result.redact();
        assert_eq!(n, 9);
        let all = format!("{result:?}");
        assert!(!all.contains(&secret), "{all}");
        assert!(!all.contains(&tail(20)), "{all}");
        // what is not a secret stays, and so does the existence of every error and finding
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.errors[1], "plain error");
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.waived.len(), 1);
        assert_eq!(result.findings[0].check_id, "rule-x");
        assert!(result
            .notes
            .last()
            .unwrap()
            .starts_with("9 value(s) were redacted"));

        // idempotent: a second pass replaces nothing, and the result (note included, once) is as it was
        let before = format!("{result:?}");
        assert_eq!(result.redact(), 0);
        assert_eq!(format!("{result:?}"), before);
        assert_eq!(
            result
                .notes
                .iter()
                .filter(|n| n.contains(NOTE_MARKER))
                .count(),
            1
        );
    }

    #[test]
    fn a_secret_split_by_an_escape_or_a_zero_width_character_is_still_removed() {
        let rest = tail(36);
        for split in ["\u{1b}[0m", "\u{200b}", "\u{feff}", "\u{ad}"] {
            let (out, n) = text(&format!("saw ghp_{split}{rest} end"));
            assert_eq!(out, "saw <redacted> end", "split: {split:?}");
            assert_eq!(n, 1);
        }
        let mut result =
            crate::RunResult::new(Vec::new(), vec![format!("error ghp_\u{1b}[31m{rest}")]);
        result.redact();
        assert!(!result.errors[0].contains(&rest), "{:?}", result.errors);
    }

    #[test]
    fn a_locations_file_and_a_projects_name_are_redacted_too() {
        let token = join(&["ghp_", &tail(36)]);
        let mut finding = fake_finding("s", "d");
        finding.location = Some(crate::finding::Location::new(
            format!("dir/{token}.txt"),
            None,
        ));
        finding.project = format!("proj-{token}");
        let mut result = crate::RunResult::new(vec![finding], Vec::new());
        assert_eq!(result.redact(), 2);
        let all = format!("{result:?}");
        assert!(!all.contains(&tail(20)), "{all}");
    }

    #[test]
    fn a_clean_result_is_left_alone_and_gets_no_note() {
        let mut result = crate::RunResult::new(vec![fake_finding("ok", "fine")], Vec::new());
        assert_eq!(result.redact(), 0);
        assert!(result.notes.is_empty());
    }

    #[test]
    fn the_run_functions_apply_the_policy_themselves() {
        let secret = join(&["TOKEN=", &tail(20)]);
        struct Noisy(String);
        impl crate::check::Check for Noisy {
            fn id(&self) -> &'static str {
                "noisy"
            }
            fn network(&self) -> crate::check::Network {
                crate::check::Network::LocalOnly
            }
            fn run(
                &self,
                _ctx: &crate::check::CheckContext,
            ) -> anyhow::Result<Vec<crate::finding::Finding>> {
                anyhow::bail!("failed: {}", self.0)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let ctx = crate::check::CheckContext::new(dir.path());
        let checks: Vec<Box<dyn crate::check::Check>> = vec![Box::new(Noisy(secret.clone()))];
        let run = |policy| {
            with_policy(policy, || {
                crate::run_checks_with(&checks, &ctx, crate::check::Tier::Fast, None)
            })
        };
        let off = run(RedactionPolicy::Off);
        assert!(
            off.errors.iter().any(|e| e.contains(&secret)),
            "{:?}",
            off.errors
        );
        let on = run(RedactionPolicy::On);
        assert!(!on.errors.is_empty(), "the failure must still be reported");
        assert!(
            on.errors.iter().all(|e| !e.contains(&tail(20))),
            "{:?}",
            on.errors
        );
    }
}
