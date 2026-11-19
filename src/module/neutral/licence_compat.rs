// SPDX-License-Identifier: MIT OR Apache-2.0
//! The licence compatibility table behind SUP-026 (LM-005), from SPDX ids only.
//!
//! **Provenance:** the data and the decision logic are a port, not a copy, of licence-tools `tools/licence_compat.py`
//! (branch `lane/compat-table`, commit 7f62d41, itself `MIT OR Apache-2.0`, the licence of this crate), whose own source is the
//! FSF/SPDX compatibility notes and each licence's text. The table is ADVISORY data, not legal advice. An id that is not in it is
//! reported as unchecked, never guessed.

/// No attribution duty, any outbound licence.
pub(super) const NO_ATTRIBUTION: &[&str] = &["CC0-1.0", "Unlicense", "0BSD", "MIT-0"];

/// Attribution required, any outbound licence, except the pairs of [`deny`].
const PERMISSIVE: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "BSL-1.0",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "BlueOak-1.0.0",
];

const GPL2: &[&str] = &["GPL-2.0-only", "GPL-2.0-or-later"];
const GPL3: &[&str] = &["GPL-3.0-only", "GPL-3.0-or-later"];
const AGPL3: &[&str] = &["AGPL-3.0-only", "AGPL-3.0-or-later"];
const LGPL21: &[&str] = &["LGPL-2.1-only", "LGPL-2.1-or-later"];
const LGPL3: &[&str] = &["LGPL-3.0-only", "LGPL-3.0-or-later"];

/// Pairs a permissive licence still cannot enter: Apache-2.0's patent terms clash with GPLv2-only and LGPL-2.1-only.
fn deny(origin: &str) -> &'static [&'static str] {
    match origin {
        "Apache-2.0" => &["GPL-2.0-only", "LGPL-2.1-only"],
        _ => &[],
    }
}

/// A copyleft origin: the outbound ids it may be carried under (itself always included); `None` when it is not a copyleft id.
fn copyleft(origin: &str) -> Option<Vec<&'static str>> {
    let join = |parts: &[&[&'static str]]| parts.iter().flat_map(|p| p.iter().copied()).collect();
    Some(match origin {
        "MPL-2.0" => join(&[&["MPL-2.0"], GPL2, LGPL21, AGPL3]),
        "LGPL-2.1-only" => join(&[&["LGPL-2.1-only"], GPL2]),
        "LGPL-2.1-or-later" => join(&[LGPL21, LGPL3, GPL2, GPL3, AGPL3]),
        "LGPL-3.0-only" => join(&[&["LGPL-3.0-only"], GPL3, AGPL3]),
        "LGPL-3.0-or-later" => join(&[LGPL3, GPL3, AGPL3]),
        "GPL-2.0-only" => vec!["GPL-2.0-only"],
        "GPL-2.0-or-later" => join(&[GPL2, GPL3, AGPL3]),
        "GPL-3.0-only" => join(&[&["GPL-3.0-only"], AGPL3]),
        "GPL-3.0-or-later" => join(&[GPL3, AGPL3]),
        "AGPL-3.0-only" => vec!["AGPL-3.0-only"],
        "AGPL-3.0-or-later" => AGPL3.to_vec(),
        _ => return None,
    })
}

/// Whether the table knows this licence id.
pub(super) fn known(lic: &str) -> bool {
    NO_ATTRIBUTION.contains(&lic) || PERMISSIVE.contains(&lic) || copyleft(lic).is_some()
}

/// Whether the origin licence asks for a holder notice (an unknown licence is treated as asking, conservatively).
pub(super) fn attribution_required(lic: &str) -> bool {
    !NO_ATTRIBUTION.contains(&lic)
}

/// The licence ids of an SPDX expression: operators and the exception after `WITH` dropped. Every id counts: `A OR B` offers
/// both, so the origin must allow both. A token is `[A-Za-z0-9.+-]+` with an optional `:` part (`DocumentRef-x:LicenseRef-y`).
pub(super) fn outbound_ids(expr: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut skip = false;
    for tok in tokens(expr) {
        if skip {
            skip = false;
        } else if tok.eq_ignore_ascii_case("WITH") {
            skip = true;
        } else if !tok.eq_ignore_ascii_case("AND") && !tok.eq_ignore_ascii_case("OR") {
            ids.push(tok);
        }
    }
    ids
}

fn tokens(expr: &str) -> Vec<String> {
    let body = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-');
    let chars: Vec<char> = expr.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !body(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && body(chars[i]) {
            i += 1;
        }
        // an optional `:` and a second run (`DocumentRef-a:LicenseRef-b`), as the reference's regex takes it
        if i < chars.len() && chars[i] == ':' {
            i += 1;
            while i < chars.len() && body(chars[i]) {
                i += 1;
            }
        }
        found.push(chars[start..i].iter().collect());
    }
    found
}

/// `(ids of outbound that origin forbids, ids that cannot be judged: an unknown origin or an unknown outbound id)`.
pub(super) fn conflicts(origin: &str, outbound: &str) -> (Vec<String>, Vec<String>) {
    let out = outbound_ids(outbound);
    if !known(origin) {
        return (Vec::new(), out);
    }
    if NO_ATTRIBUTION.contains(&origin) {
        return (Vec::new(), Vec::new());
    }
    if PERMISSIVE.contains(&origin) {
        let bad = out
            .into_iter()
            .filter(|o| deny(origin).contains(&o.as_str()))
            .collect();
        return (bad, Vec::new());
    }
    let allowed = copyleft(origin).unwrap_or_default();
    let bad = out
        .iter()
        .filter(|o| known(o) && !allowed.contains(&o.as_str()))
        .cloned()
        .collect();
    let unknown = out.iter().filter(|o| !known(o)).cloned().collect();
    (bad, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_origin_allows_any_outbound_but_the_apache_pairs() {
        assert_eq!(conflicts("MIT", "MIT OR Apache-2.0"), (vec![], vec![]));
        assert_eq!(
            conflicts("Apache-2.0", "GPL-2.0-only").0,
            vec!["GPL-2.0-only"]
        );
        assert!(conflicts("Apache-2.0", "GPL-3.0-only").0.is_empty());
    }

    #[test]
    fn copyleft_origin_forbids_what_its_table_does_not_list() {
        assert_eq!(conflicts("GPL-3.0-only", "MIT").0, vec!["MIT"]);
        assert!(conflicts("GPL-3.0-or-later", "AGPL-3.0-or-later")
            .0
            .is_empty());
        assert_eq!(
            conflicts("GPL-3.0-only", "GPL-3.0-only OR MIT").0,
            vec!["MIT"]
        );
        assert!(conflicts("MPL-2.0", "LGPL-2.1-or-later").0.is_empty());
        assert_eq!(conflicts("MPL-2.0", "MIT").0, vec!["MIT"]);
    }

    #[test]
    fn the_exception_after_with_is_not_a_licence() {
        assert_eq!(
            outbound_ids("GPL-3.0-only WITH Classpath-exception-2.0"),
            vec!["GPL-3.0-only"]
        );
        assert!(
            conflicts("GPL-3.0-only", "GPL-3.0-only WITH Classpath-exception-2.0")
                .0
                .is_empty()
        );
    }

    #[test]
    fn an_unknown_id_is_unjudged_not_guessed() {
        assert_eq!(conflicts("Weird-1.0", "MIT"), (vec![], vec!["MIT".into()]));
        let (bad, unknown) = conflicts("GPL-3.0-only", "LicenseRef-Proprietary");
        assert!(bad.is_empty());
        assert_eq!(unknown, vec!["LicenseRef-Proprietary"]);
        assert!(!known("Weird-1.0") && known("MIT-0") && !attribution_required("MIT-0"));
        assert!(attribution_required("Weird-1.0"));
    }
}
