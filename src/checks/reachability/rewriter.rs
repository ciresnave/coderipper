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
    let mut guard = ProcMacroGuard::default();
    source
        .lines()
        .map(|line| {
            if guard.is_proc_macro_entry_point(line) {
                line.to_string()
            } else {
                rewrite_line(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if source.ends_with('\n') { "\n" } else { "" }
}

/// Finds the item line of a procedural macro's entry point (`#[proc_macro]`, `#[proc_macro_derive(..)]`,
/// `#[proc_macro_attribute]`). rustc requires such a function to stay `pub` ("functions tagged with
/// `#[proc_macro_derive]` must be `pub`"), so downgrading it turns every proc-macro crate into a
/// compile error and the check into "could not run". A proc-macro crate can export nothing but these
/// functions, so leaving them alone hides nothing: its other `pub` items are still downgraded.
///
/// Line-based like the rewrite itself, but it reads past what can sit between the attribute and the
/// item: other attributes, `//` and `/* */` comments, doc comments, blank lines, an attribute spread
/// over several lines (brackets inside string literals and after `//` do not count), and code after
/// the attribute's closing bracket on the same line (then the armed attribute is spent on that line,
/// never on a later item). An attribute that never closes (unbalanced brackets) is given up on at the
/// next line that starts with `pub ` or `#[`, so one odd line cannot hide the rest of a file. Not
/// handled: the attribute inside a `cfg_attr`.
#[derive(Default)]
struct ProcMacroGuard {
    /// Square-bracket depth of an attribute that has not closed yet.
    open_brackets: i32,
    /// A proc-macro attribute was seen and its item line has not arrived.
    armed: bool,
    /// Inside a `/* ... */` comment that began on an earlier line.
    in_block_comment: bool,
}

impl ProcMacroGuard {
    /// Feeds one line in file order; true when it is the item line a proc-macro attribute governs.
    fn is_proc_macro_entry_point(&mut self, line: &str) -> bool {
        let mut text = line.trim_start();
        loop {
            if self.in_block_comment {
                let Some(end) = text.find("*/") else {
                    return false;
                };
                self.in_block_comment = false;
                text = text[end + 2..].trim_start();
            }
            if text.is_empty() || text.starts_with("//") {
                return false;
            }
            if let Some(after) = text.strip_prefix("/*") {
                self.in_block_comment = true;
                text = after;
                continue;
            }
            if self.open_brackets > 0 {
                if text.starts_with("pub ") || text.starts_with("#[") {
                    // Unbalanced brackets: this cannot be the inside of an attribute.
                    self.open_brackets = 0;
                    continue;
                }
                match close_attribute(text, self.open_brackets) {
                    Ok(end) => {
                        self.open_brackets = 0;
                        text = text[end..].trim_start();
                        continue;
                    }
                    Err(depth) => {
                        self.open_brackets = depth;
                        return false;
                    }
                }
            }
            if text.starts_with("#[") || text.starts_with("#![") {
                if is_proc_macro_attribute(text) {
                    self.armed = true;
                }
                match close_attribute(text, 0) {
                    Ok(end) => {
                        text = text[end..].trim_start();
                        continue;
                    }
                    Err(depth) => {
                        self.open_brackets = depth;
                        return false;
                    }
                }
            }
            // Code. If an armed attribute precedes it, this is the item it governs.
            return std::mem::take(&mut self.armed);
        }
    }
}

/// Scans `text` for the `]` that closes an attribute whose bracket depth is `depth` at its start.
/// `Ok(i)`: the byte index just past that `]`. `Err(d)`: the line ended (or a `//` comment began) at
/// depth `d`. Brackets inside string literals are not counted.
fn close_attribute(text: &str, mut depth: i32) -> Result<usize, i32> {
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '/' if matches!(chars.peek(), Some((_, '/'))) => return Err(depth),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth <= 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
    }
    Err(depth)
}

/// `#[proc_macro]`, `#[proc_macro_derive(..)]`, `#[proc_macro_attribute]`, and nothing that merely
/// starts with those letters.
fn is_proc_macro_attribute(trimmed: &str) -> bool {
    let Some(inner) = trimmed.strip_prefix("#[") else {
        return false;
    };
    let inner = inner.trim_start();
    ["proc_macro_attribute", "proc_macro_derive", "proc_macro"]
        .iter()
        .any(|name| {
            inner.strip_prefix(name).is_some_and(|rest| {
                !rest
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
        })
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
    fn a_proc_macro_entry_point_stays_pub() {
        // rustc: "functions tagged with `#[proc_macro_derive]` must be `pub`". Downgrading one made
        // every proc-macro crate a compile error (issue 54: vulkane_derive).
        for attr in [
            "#[proc_macro]",
            "#[proc_macro_derive(Hello)]",
            "#[proc_macro_derive(Hello, attributes(hello))]",
            "#[proc_macro_attribute]",
            "  #[ proc_macro_derive(Hello) ]",
        ] {
            let src = format!("{attr}\npub fn entry(i: TokenStream) -> TokenStream {{ i }}\n");
            assert_eq!(rewrite_pub_to_pub_crate(&src), src, "attribute: {attr}");
        }
    }

    #[test]
    fn a_proc_macro_attribute_can_span_lines_and_be_followed_by_other_attributes() {
        let multi = "#[proc_macro_derive(\n    Hello,\n    attributes(hello, world)\n)]\npub fn entry(i: TokenStream) -> TokenStream { i }\n";
        assert_eq!(rewrite_pub_to_pub_crate(multi), multi);

        let stacked = "/// Docs.\n#[proc_macro_derive(Hello)]\n#[inline]\n\n// a comment\n/// more docs\n#[allow(non_snake_case)]\npub fn entry(i: TokenStream) -> TokenStream { i }\n";
        assert_eq!(rewrite_pub_to_pub_crate(stacked), stacked);
    }

    #[test]
    fn only_the_function_a_proc_macro_attribute_governs_stays_pub() {
        // The guard must be spent on the item line: the helper after it is an ordinary pub item.
        let src = "#[proc_macro_derive(Hello)]\npub fn entry(i: TokenStream) -> TokenStream { i }\npub fn helper() {}\npub struct Thing;\n";
        let want = "#[proc_macro_derive(Hello)]\npub fn entry(i: TokenStream) -> TokenStream { i }\npub(crate) fn helper() {}\npub(crate) struct Thing;\n";
        assert_eq!(rewrite_pub_to_pub_crate(src), want);
    }

    #[test]
    fn an_attribute_that_only_looks_like_a_proc_macro_one_does_not_protect() {
        for attr in [
            "#[proc_macro_derived(Hello)]",
            "#[my_proc_macro]",
            "#[proc_macros]",
            "#[derive(Debug)]",
        ] {
            let src = format!("{attr}\npub fn f() {{}}\n");
            let want = format!("{attr}\npub(crate) fn f() {{}}\n");
            assert_eq!(rewrite_pub_to_pub_crate(&src), want, "attribute: {attr}");
        }
    }

    #[test]
    fn code_after_the_attribute_on_the_same_line_does_not_arm_the_next_item() {
        // Audit finding: `#[proc_macro] pub fn a() {}` on one line used to leave the guard armed, so
        // the NEXT item stayed `pub` and a dead helper there was silently not reported.
        let one_line =
            "#[proc_macro] pub fn a(i: TokenStream) -> TokenStream { i }\npub fn dead() {}\n";
        let want = "#[proc_macro] pub fn a(i: TokenStream) -> TokenStream { i }\npub(crate) fn dead() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(one_line), want);

        let closing_line = "#[proc_macro_derive(\n    A\n)] pub fn f() {}\npub fn dead() {}\n";
        let want = "#[proc_macro_derive(\n    A\n)] pub fn f() {}\npub(crate) fn dead() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(closing_line), want);
    }

    #[test]
    fn comments_between_the_attribute_and_the_function_are_skipped() {
        let src = "#[proc_macro_derive(A)]\n/** docs\n * more\n */\n/* inline */\n// line\npub fn f() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(src), src);
    }

    #[test]
    fn brackets_in_strings_and_trailing_comments_do_not_confuse_the_attribute() {
        for src in [
            "#[doc = \"[\"]\n#[proc_macro]\npub fn f() {}\n",
            "#[proc_macro] // see [1\npub fn f() {}\n",
            "#[proc_macro_derive(A, attributes(x))] // ]\npub fn f() {}\n",
        ] {
            assert_eq!(rewrite_pub_to_pub_crate(src), src, "case: {src:?}");
        }
    }

    #[test]
    fn an_attribute_that_never_closes_does_not_hide_the_rest_of_the_file() {
        // Unbalanced brackets must not turn every later line into "attribute continuation".
        let src = "#[cfg(any(\npub fn x() {}\n#[proc_macro]\npub fn entry() {}\npub fn dead() {}\n";
        let want = "#[cfg(any(\npub(crate) fn x() {}\n#[proc_macro]\npub fn entry() {}\npub(crate) fn dead() {}\n";
        assert_eq!(rewrite_pub_to_pub_crate(src), want);
    }

    #[test]
    fn an_inner_attribute_does_not_arm_the_guard() {
        let src = "#![allow(dead_code)]\npub fn f() {}\n";
        assert_eq!(
            rewrite_pub_to_pub_crate(src),
            "#![allow(dead_code)]\npub(crate) fn f() {}\n"
        );
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
