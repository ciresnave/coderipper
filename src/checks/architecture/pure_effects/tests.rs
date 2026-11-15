// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn entries(extra: &[&str]) -> Vec<Vec<String>> {
    let mut a = ArchitectureConfig::default();
    if !extra.is_empty() {
        a.effects = Some(architecture::EffectsDecl {
            add: extra.iter().map(|s| (*s).to_string()).collect(),
            remove: Vec::new(),
        });
    }
    a.effect_list()
        .iter()
        .map(|e| e.split("::").map(str::to_string).collect())
        .collect()
}

fn run(text: &str, extra: &[&str]) -> (Vec<Hit>, usize) {
    let f = SourceFile {
        path: "src/lib.rs".into(),
        text: text.into(),
    };
    scan(&f, &entries(extra)).unwrap()
}

fn paths(h: &[Hit]) -> Vec<String> {
    h.iter().map(|x| x.path.clone()).collect()
}

#[test]
fn a_file_call_through_use_and_a_full_path_are_both_found() {
    let (h, _) = run(
        "use std::fs;\npub fn a() -> String {\n    fs::read_to_string(\"x\").unwrap()\n}\npub fn b() {\n    std::env::var(\"HOME\").ok();\n}\n",
        &[],
    );
    assert_eq!(
        paths(&h),
        ["std::fs::read_to_string", "std::env::var"],
        "{h:#?}"
    );
    assert_eq!((h[0].line, h[0].entry.as_str()), (3, "std::fs"));
}

#[test]
fn a_renamed_import_is_still_seen() {
    let (h, _) = run(
        "use std::time::Instant as Clock;\npub fn t() {\n    let _ = Clock::now();\n}\n",
        &[],
    );
    assert_eq!(paths(&h), ["std::time::Instant::now"], "{h:#?}");
}

#[test]
fn each_family_of_the_list_is_found() {
    let (h, _) = run(
        "use rand::Rng;\nuse std::net::TcpStream;\nuse std::process::Command;\nuse std::time::SystemTime;\nuse std::thread;\nuse tokio::spawn;\nuse chrono::Utc;\n\
         pub fn f(_: TcpStream) {\n    let _ = Command::new(\"x\");\n    let _ = SystemTime::now();\n    thread::sleep(std::time::Duration::ZERO);\n    spawn(async {});\n    let _ = Utc::now();\n    let _ = rand::random::<u8>();\n}\n",
        &[],
    );
    let p = paths(&h);
    for want in [
        "std::net::TcpStream",
        "std::process::Command::new",
        "std::time::SystemTime::now",
        "std::thread::sleep",
        "tokio::spawn",
        "chrono::Utc::now",
        "rand::random",
    ] {
        assert!(p.iter().any(|x| x == want), "{want} in {p:?}");
    }
    assert!(
        !p.iter().any(|x| x.contains("Duration")),
        "Duration is not an effect: {p:?}"
    );
}

#[test]
fn pure_code_is_clean() {
    let (h, g) = run(
        "use std::collections::BTreeMap;\nuse std::time::Duration;\npub fn price(m: &BTreeMap<String, u64>, d: Duration) -> u64 {\n    m.values().sum::<u64>() + d.as_secs()\n}\n",
        &[],
    );
    assert!(h.is_empty() && g == 0, "{h:#?}");
}

#[test]
fn an_import_alone_is_not_an_effect_and_test_code_is_skipped() {
    let (h, _) = run(
        "use std::fs;\n#[cfg(test)]\nmod tests {\n    use std::fs;\n    #[test]\n    fn t() { fs::read(\"x\").unwrap(); }\n}\n#[cfg(test)]\nfn helper() { std::env::var(\"X\").ok(); }\npub struct S;\nimpl S {\n    #[test]\n    fn u() { std::process::exit(1); }\n}\n",
        &[],
    );
    assert!(h.is_empty(), "{h:#?}");
}

#[test]
fn an_added_entry_is_found_and_a_crate_or_self_path_is_not() {
    let text = "use metrics::counter;\nmod inner {\n    pub fn a() {}\n}\npub fn f() {\n    counter(\"x\");\n    crate::inner::a();\n    self::inner::a();\n}\n";
    assert!(run(text, &[]).0.is_empty());
    let (h, _) = run(text, &["metrics"]);
    assert_eq!(paths(&h), ["metrics::counter"], "{h:#?}");
}

#[test]
fn a_type_in_a_signature_and_a_struct_literal_count() {
    let (h, _) = run(
        "use std::fs::File;\npub struct Cfg {\n    pub f: File,\n}\npub fn open(f: &File) {}\n",
        &[],
    );
    assert_eq!(h.len(), 2, "{h:#?}");
}

#[test]
fn a_glob_import_of_an_effect_namespace_is_counted_not_resolved() {
    let (h, g) = run(
        "use std::fs::*;\npub fn f() {\n    let _ = read(\"x\");\n}\n",
        &[],
    );
    assert!(h.is_empty(), "names behind a glob are not resolved: {h:#?}");
    assert_eq!(g, 1);
}

#[test]
fn a_file_that_does_not_parse_is_an_error_never_clean() {
    let f = SourceFile {
        path: "src/lib.rs".into(),
        text: "fn (".into(),
    };
    let e = scan(&f, &entries(&[])).unwrap_err().to_string();
    assert!(
        e.contains("src/lib.rs") && e.contains("refusing to guess"),
        "{e}"
    );
}

#[test]
fn the_finding_names_the_component_the_effect_and_the_line() {
    let h = Hit {
        line: 7,
        path: "std::fs::read".into(),
        entry: "std::fs".into(),
    };
    let f = finding(
        "pricing",
        "pricing/Cargo.toml",
        "pricing/src/lib.rs",
        &h,
        "p",
    );
    assert_eq!(f.subject.as_deref(), Some("pricing:std::fs::read"));
    assert_eq!(f.location.as_ref().and_then(|l| l.line), Some(7));
    assert!(f.summary.contains("pricing") && f.detail.contains("not referentially pure"));
}
