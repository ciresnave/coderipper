// SPDX-License-Identifier: MIT OR Apache-2.0
//! COR-003: a codec pair has a round-trip property test.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. A *codec pair* is two public functions or
//! methods of one scope (the same `impl` self type, or the same module) whose names are a verb pair, optionally with the same suffix:
//! `encode`/`decode`, `serialize`/`deserialize`, `to_bytes`/`from_bytes`, `to_json`/`from_json`, `pack`/`unpack`,
//! `compress`/`decompress`, `encrypt`/`decrypt`, `marshal`/`unmarshal` (`encode_header`/`decode_header` is a pair; `encode_header` and
//! `decode_body` are not). The methods of a trait impl count as public.
//!
//! A pair is *covered* when some `.rs` file of the package (its tests and doc tests included, in any directory) names a property-test
//! library (`proptest`, `quickcheck`, `prop_assert`, `bolero`, `test_strategy`) and holds both function names as whole words. A pair
//! that is not covered gets one **Medium**-severity, Low-confidence finding, at the encoder.
//!
//! **What it cannot see:** a pair named another way, a codec given by a trait pair (`impl Encoder` and `impl Decoder` on different
//! types), a pair split across scopes, a property test that reaches both functions through a helper, and whether the property asserts
//! the round trip. The second half of the rule (repeating normalization is stable) is not judged. A file `syn` cannot parse is named in
//! a note, never read as clean.

use super::rust_package::{load, unparsed_note, Package};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use std::collections::BTreeMap;
use syn::visit::Visit;

const ID: &str = crate::rule_id::COR_003;

/// The verb pairs: the encoding side and the decoding side.
const PAIRS: &[(&str, &str)] = &[
    ("encode", "decode"),
    ("serialize", "deserialize"),
    ("to_bytes", "from_bytes"),
    ("to_json", "from_json"),
    ("pack", "unpack"),
    ("compress", "decompress"),
    ("encrypt", "decrypt"),
    ("marshal", "unmarshal"),
];

/// What names a property-test library in a file.
const MARKERS: &[&str] = &[
    "proptest",
    "quickcheck",
    "prop_assert",
    "bolero",
    "test_strategy",
];

/// A function of a scope: its name, its line, and whether a caller outside the module can see it.
struct Func {
    name: String,
    line: u32,
    public: bool,
}

#[derive(Default)]
struct Walk {
    modules: Vec<String>,
    scopes: BTreeMap<String, Vec<Func>>,
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map_or_else(|| "?".into(), |s| s.ident.to_string()),
        syn::Type::Reference(r) => type_name(&r.elem),
        _ => "?".into(),
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
}

fn is_public(vis: &syn::Visibility) -> bool {
    !matches!(vis, syn::Visibility::Inherited)
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        self.modules.push(i.ident.to_string());
        syn::visit::visit_item_mod(self, i);
        self.modules.pop();
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        let scope = format!("mod {}", self.modules.join("::"));
        self.scopes.entry(scope).or_default().push(Func {
            name: i.sig.ident.to_string(),
            line: u32::try_from(i.sig.ident.span().start().line).unwrap_or(0),
            public: is_public(&i.vis),
        });
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        let scope = format!("type {}", type_name(&i.self_ty));
        let of_trait = i.trait_.is_some();
        for item in &i.items {
            if let syn::ImplItem::Fn(f) = item {
                if is_cfg_test(&f.attrs) || has_test_attr(&f.attrs) {
                    continue;
                }
                self.scopes.entry(scope.clone()).or_default().push(Func {
                    name: f.sig.ident.to_string(),
                    line: u32::try_from(f.sig.ident.span().start().line).unwrap_or(0),
                    public: of_trait || is_public(&f.vis),
                });
            }
        }
    }
}

/// The suffix of `name` after `verb` (`""` for the bare verb), or `None` when `name` is not that verb.
fn suffix<'a>(name: &'a str, verb: &str) -> Option<&'a str> {
    if name == verb {
        Some("")
    } else {
        name.strip_prefix(verb)?.strip_prefix('_')
    }
}

/// A pair found in one scope: the encoder and the decoder.
struct Pair {
    scope: String,
    encoder: String,
    decoder: String,
    line: u32,
}

fn pairs(scopes: &BTreeMap<String, Vec<Func>>) -> Vec<Pair> {
    let mut out = Vec::new();
    for (scope, funcs) in scopes {
        for enc in funcs.iter().filter(|f| f.public) {
            for (ev, dv) in PAIRS {
                let Some(suf) = suffix(&enc.name, ev) else {
                    continue;
                };
                let want = if suf.is_empty() {
                    (*dv).to_string()
                } else {
                    format!("{dv}_{suf}")
                };
                if funcs.iter().any(|f| f.public && f.name == want) {
                    out.push(Pair {
                        scope: scope.clone(),
                        encoder: enc.name.clone(),
                        decoder: want,
                        line: enc.line,
                    });
                }
            }
        }
    }
    out
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn has_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(word).any(|(i, _)| {
        let before = i == 0 || !is_word_byte(bytes[i - 1]);
        let end = i + word.len();
        let after = end >= bytes.len() || !is_word_byte(bytes[end]);
        before && after
    })
}

/// The texts of the package's `.rs` files that name a property-test library: every tracked file under the package directory,
/// the test directories included.
fn property_texts(view: &RepoView, package: &Package) -> Vec<String> {
    let prefix = if package.dir.is_empty() {
        String::new()
    } else {
        format!("{}/", package.dir)
    };
    view.paths()
        .iter()
        .filter(|p| p.ends_with(".rs") && p.starts_with(&prefix))
        .filter_map(|p| view.read(p))
        .filter(|t| MARKERS.iter().any(|m| t.contains(m)))
        .collect()
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return Verdict::NotApplicable(
            "the repository tracks no Rust package source outside tests, examples, tools and vendored trees".into(),
        );
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        let texts = property_texts(view, p);
        for s in &p.sources {
            let mut walk = Walk::default();
            walk.visit_file(&s.file);
            for pair in pairs(&walk.scopes) {
                let covered = texts
                    .iter()
                    .any(|t| has_word(t, &pair.encoder) && has_word(t, &pair.decoder));
                if covered {
                    continue;
                }
                let subject = format!(
                    "{}:{}:{}/{}",
                    s.path, pair.scope, pair.encoder, pair.decoder
                );
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Low,
                        view.name(),
                        format!(
                            "`{}` and `{}` ({}) are an encoder and decoder with no round-trip property test",
                            pair.encoder, pair.decoder, pair.scope
                        ),
                        "Example-based tests rarely reach the odd inputs where a codec loses or corrupts data. Add a property test (proptest or quickcheck) that generates a value, encodes it, decodes the result and asserts it equals the original, for the input domain the codec supports.",
                    )
                    .location(Location::new(s.path.as_str(), Some(pair.line)))
                    .subject(subject)
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that pairs functions, and {} file(s) naming a property-test library were searched",
                        texts.len()
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found_in(files: &[(&str, &str)]) -> Vec<Finding> {
        let mut all = vec![("Cargo.toml", MANIFEST)];
        all.extend_from_slice(files);
        match judge(&RepoView::from_files(&all)) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    const CODEC: &str = "pub struct C;
impl C {
    pub fn encode(v: &[u8]) -> Vec<u8> { v.to_vec() }
    pub fn decode(v: &[u8]) -> Vec<u8> { v.to_vec() }
}
";

    #[test]
    fn a_public_pair_with_no_property_test_is_reported_at_the_encoder() {
        let f = found_in(&[("src/lib.rs", CODEC)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].location.as_ref().unwrap().line, Some(3));
        assert!(f[0]
            .subject
            .as_deref()
            .unwrap()
            .ends_with("type C:encode/decode"));
    }

    #[test]
    fn a_property_test_that_names_both_functions_covers_the_pair_wherever_it_lives() {
        let inline = format!(
            "{CODEC}#[cfg(test)]\nmod t {{ proptest::proptest! {{ fn rt(x: Vec<u8>) {{ assert_eq!(C::decode(&C::encode(&x)), x); }} }} }}\n"
        );
        assert!(found_in(&[("src/lib.rs", &inline)]).is_empty());
        let separate = "use proptest::prelude::*;\nproptest! { fn rt(x: Vec<u8>) { assert_eq!(p::C::decode(&p::C::encode(&x)), x); } }\n";
        assert!(found_in(&[("src/lib.rs", CODEC), ("tests/rt.rs", separate)]).is_empty());
    }

    #[test]
    fn a_property_test_of_only_one_side_or_of_another_pair_does_not_cover() {
        let one = "use proptest::prelude::*;\nproptest! { fn e(x: Vec<u8>) { C::encode(&x); } }\n";
        assert_eq!(
            found_in(&[("src/lib.rs", CODEC), ("tests/e.rs", one)]).len(),
            1
        );
    }

    #[test]
    fn suffixed_names_pair_only_with_the_same_suffix_and_private_or_split_pairs_are_not_seen() {
        let src = "pub fn encode_header() {}\npub fn decode_header() {}\npub fn encode_body() {}\npub fn decode_footer() {}\nfn pack() {}\nfn unpack() {}\npub struct A; impl A { pub fn to_bytes(&self) {} }\npub struct B; impl B { pub fn from_bytes() {} }\n";
        let f = found_in(&[("src/lib.rs", src)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("encode_header"), "{}", f[0].summary);
    }

    #[test]
    fn trait_impl_methods_pair_and_test_code_is_not_read() {
        let src = "pub struct S;\nimpl Ser for S { fn serialize(&self) {} }\nimpl De for S { fn deserialize() {} }\n#[cfg(test)]\nmod t { pub fn encode() {} pub fn decode() {} }\n";
        let f = found_in(&[("src/lib.rs", src)]);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0]
            .subject
            .as_deref()
            .unwrap()
            .contains("serialize/deserialize"));
    }

    #[test]
    fn every_finding_validates() {
        for f in found_in(&[("src/lib.rs", CODEC)]) {
            f.validate().unwrap();
        }
    }
}
