//! Line-based `pub` -> `pub(crate)` rewrite (design doc §2, plan's Global Constraints: intentionally
//! not a full AST rewrite for v1). Downgrades a bare `pub` immediately before one of the declaring
//! keywords, OR before `use` (a re-export); leaves `pub(crate)`, `pub(super)`, `pub(in ...)` alone.
//! `pub use` is downgraded too -- confirmed empirically (ledger) that leaving it at full `pub` while
//! its target drops to `pub(crate)` is a hard compile error (E0364), and that `pub(crate) use` is
//! always safe (internal imports through it still resolve; external ones correctly stop, which is
//! the point).

const KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "const", "static", "type", "mod", "use",
];

pub fn rewrite_pub_to_pub_crate(source: &str) -> String {
    source
        .lines()
        .map(rewrite_line)
        .collect::<Vec<_>>()
        .join("\n")
        + if source.ends_with('\n') { "\n" } else { "" }
}

fn rewrite_line(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);

    let Some(after_pub) = rest.strip_prefix("pub ") else {
        return line.to_string();
    };
    // Already scoped (`pub(crate)`, `pub(super)`, `pub(in ...)`) -- leave alone. `after_pub` starts
    // right after "pub ", so a scoped visibility shows up as "(...)" here, and `pub(...)` with no
    // space (e.g. "pub(crate)") never matched the "pub " prefix at all.

    if KEYWORDS.contains(&next_declaring_keyword(after_pub)) {
        format!("{indent}pub(crate) {after_pub}")
    } else {
        line.to_string()
    }
}

/// Skips leading qualifiers (`async`, `unsafe`, `extern "ABI"`) to find the real declaring
/// keyword -- "pub async fn x()" must still be recognized as a `fn` declaration, not left at full
/// `pub` just because "async" isn't itself in `KEYWORDS`.
fn next_declaring_keyword(after_pub: &str) -> &str {
    let mut probe = after_pub;
    loop {
        let word = probe.split_whitespace().next().unwrap_or("");
        match word {
            "async" | "unsafe" => probe = probe[word.len()..].trim_start(),
            "extern" => {
                probe = probe[word.len()..].trim_start();
                if probe.starts_with('"') {
                    if let Some(end) = probe[1..].find('"') {
                        probe = probe[end + 2..].trim_start();
                    }
                }
            }
            _ => break,
        }
    }
    probe.split_whitespace().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::rewrite_pub_to_pub_crate;

    #[test]
    fn a_bare_pub_fn_is_downgraded() {
        let src = "pub fn dead() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(src), "pub(crate) fn dead() {}\n");
    }

    #[test]
    fn pub_use_is_downgraded_too() {
        // Confirmed empirically (see ledger): leaving `pub use` at full `pub` while its target
        // becomes `pub(crate)` is a HARD COMPILE ERROR (E0364) -- Rust never allows a re-export to
        // be more visible than the thing it re-exports. `pub(crate) use` is always safe: internal
        // imports through it still resolve (pub(crate) is crate-wide), and external ones correctly
        // stop working, which is exactly what testing INTERNAL reachability requires.
        let src = "pub use crate::inner::Thing;\n";
        assert_eq!(
            rewrite_pub_to_pub_crate(src),
            "pub(crate) use crate::inner::Thing;\n"
        );
    }

    #[test]
    fn already_scoped_visibility_is_left_alone() {
        let src = "pub(crate) fn a() {}\npub(super) fn b() {}\npub(in crate::x) fn c() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(src), src);
    }

    #[test]
    fn indented_pub_items_inside_a_module_are_downgraded() {
        let src = "mod inner {\n    pub struct Thing;\n}\n";
        assert_eq!(
            rewrite_pub_to_pub_crate(src),
            "mod inner {\n    pub(crate) struct Thing;\n}\n"
        );
    }

    #[test]
    fn qualified_fn_declarations_are_still_recognized() {
        // Review finding: the old code only checked the word IMMEDIATELY after "pub ", so
        // "pub async fn"/"pub unsafe fn"/"pub extern "C" fn" were silently left at full `pub`
        // (never analyzed at all, since "async"/"unsafe"/"extern" aren't declaring keywords).
        let cases = [
            ("pub async fn f() {}\n", "pub(crate) async fn f() {}\n"),
            ("pub unsafe fn f() {}\n", "pub(crate) unsafe fn f() {}\n"),
            (
                "pub extern \"C\" fn f() {}\n",
                "pub(crate) extern \"C\" fn f() {}\n",
            ),
            (
                "pub unsafe extern \"C\" fn f() {}\n",
                "pub(crate) unsafe extern \"C\" fn f() {}\n",
            ),
        ];
        for (src, want) in cases {
            assert_eq!(rewrite_pub_to_pub_crate(src), want, "case: {src:?}");
        }
    }

    #[test]
    fn every_declaring_keyword_is_covered() {
        for kw in [
            "fn", "struct", "enum", "trait", "const", "static", "type", "mod",
        ] {
            let src = format!("pub {kw} x;\n");
            let want = format!("pub(crate) {kw} x;\n");
            assert_eq!(rewrite_pub_to_pub_crate(&src), want, "keyword {kw}");
        }
    }
}
