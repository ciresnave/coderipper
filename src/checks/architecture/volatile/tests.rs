// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn decl(names: &[&str], namespaces: &[&str], cfg: bool) -> VolatileDecl {
    VolatileDecl {
        names: names.iter().map(|s| (*s).into()).collect(),
        namespaces: namespaces.iter().map(|s| (*s).into()).collect(),
        feature_cfg_in_policy: cfg,
    }
}

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

#[test]
fn a_volatile_name_is_counted_wherever_it_appears_as_an_identifier() {
    let v = decl(&["BackendId"], &[], false);
    let c = count(
        &[file(
            "src/a.rs",
            "pub fn pick(b: BackendId) -> Option<BackendId> { match b { BackendId::Cpu => None, other => Some(other) } }\nstruct S { id: BackendId }\n",
        )],
        &v,
    )
    .unwrap();
    assert_eq!(c.names.get("BackendId"), Some(&4));
    assert_eq!(c.per_file.get("src/a.rs"), Some(&4));
}

#[test]
fn a_name_inside_a_string_or_a_comment_is_not_counted() {
    let v = decl(&["BackendId"], &["sqlx::"], false);
    let c = count(
        &[file(
            "src/a.rs",
            "// BackendId and sqlx::Pool\nfn f() -> &'static str { \"BackendId sqlx::Pool\" }\n",
        )],
        &v,
    )
    .unwrap();
    assert_eq!(c.uses(), 0, "{c:?}");
    // positive control: the same declaration finds the real thing
    let c = count(
        &[file("src/a.rs", "use sqlx::Pool; fn g(_: BackendId) {}")],
        &v,
    )
    .unwrap();
    assert_eq!(c.uses(), 2, "{c:?}");
}

#[test]
fn a_namespace_matches_the_path_prefix_and_not_a_longer_identifier() {
    let v = decl(&[], &["sqlx::", "tokio::net::"], false);
    let c = count(
        &[file(
            "src/a.rs",
            "use sqlx::PgPool;\nuse tokio::net::TcpStream;\nuse tokio::sync::Mutex;\nuse not_sqlx::Thing;\nfn f() { let x = sqlx::query(\"\"); let _ = x; }\n",
        )],
        &v,
    )
    .unwrap();
    assert_eq!(c.namespaces.get("sqlx::"), Some(&2), "{c:?}");
    assert_eq!(c.namespaces.get("tokio::net::"), Some(&1), "{c:?}");
}

#[test]
fn feature_cfg_attributes_are_counted_per_feature_and_only_when_asked() {
    let src = "#[cfg(feature = \"cuda\")]\nmod cuda;\n#[cfg(all(feature = \"metal\", not(feature = \"cuda\")))]\nfn m() {}\n#[cfg(unix)]\nfn u() {}\nstruct S { #[cfg(feature = \"cuda\")] a: u8 }\n";
    let on = count(&[file("src/a.rs", src)], &decl(&[], &[], true)).unwrap();
    assert_eq!(on.cfg_lines, 3, "{on:?}");
    assert_eq!(on.features.get("cuda"), Some(&3), "{on:?}");
    assert_eq!(on.features.get("metal"), Some(&1), "{on:?}");
    let off = count(&[file("src/a.rs", src)], &decl(&[], &[], false)).unwrap();
    assert_eq!(off.cfg_lines, 0, "{off:?}");
}

#[test]
fn test_items_are_not_counted() {
    let v = decl(&["BackendId"], &["sqlx::"], true);
    let c = count(
        &[file(
            "src/a.rs",
            "#[cfg(test)]\nmod tests { use sqlx::Pool; fn t(_: BackendId) {} #[cfg(feature = \"x\")] fn y() {} }\nfn real() {}\n",
        )],
        &v,
    )
    .unwrap();
    assert_eq!(c, Counts::default(), "{c:?}");
}

#[test]
fn an_inline_module_is_counted_item_by_item_without_double_counting() {
    let v = decl(&["BackendId"], &[], false);
    let c = count(
        &[file(
            "src/a.rs",
            "mod inner { pub fn f(_: BackendId) { fn nested(_: BackendId) {} } }\n",
        )],
        &v,
    )
    .unwrap();
    assert_eq!(c.names.get("BackendId"), Some(&2), "{c:?}");
}

#[test]
fn a_file_that_does_not_parse_is_an_error_not_a_smaller_count() {
    let e = count(&[file("src/bad.rs", "fn (")], &decl(&["X"], &[], false)).unwrap_err();
    assert!(e.to_string().contains("src/bad.rs"), "{e}");
}

fn tally(name: &str, c: Counts) -> (String, Option<String>, Counts) {
    (name.into(), Some(format!("{name}/Cargo.toml")), c)
}

#[test]
fn a_component_with_uses_is_one_low_finding_and_a_cfg_only_component_is_info() {
    let v = decl(&["BackendId"], &[], true);
    let mut used = Counts::default();
    used.names.insert("BackendId".into(), 7);
    used.cfg_lines = 2;
    used.features.insert("cuda".into(), 2);
    used.per_file.insert("fuel-dispatch/src/a.rs".into(), 9);
    let mut cfg_only = Counts {
        cfg_lines: 1,
        ..Counts::default()
    };
    cfg_only.features.insert("metal".into(), 1);
    let (f, notes) = judge(
        vec![
            tally("fuel-dispatch", used),
            tally("fuel-ir", cfg_only),
            tally("fuel-clean", Counts::default()),
        ],
        &v,
        "fuel",
        Vec::new(),
    );
    assert_eq!(f.len(), 2, "{f:?}");
    let by = |s: &str| f.iter().find(|x| x.subject.as_deref() == Some(s)).unwrap();
    assert_eq!(by("fuel-dispatch").severity, Severity::Low);
    assert_eq!(by("fuel-ir").severity, Severity::Info);
    assert!(by("fuel-dispatch").summary.contains("7 use(s)"));
    let ranking = notes.iter().find(|n| n.contains("ranking")).unwrap();
    assert!(
        ranking.find("fuel-dispatch").unwrap() < ranking.find("fuel-ir").unwrap(),
        "{ranking}"
    );
    assert!(ranking.contains("fuel-clean 0/0"), "{ranking}");
}

#[test]
fn only_a_stable_policy_component_is_judged() {
    let a = |extra: &str| {
        crate::config::ConfigFile::parse(
            &format!("[architecture]\n[architecture.volatile]\nnames = [\"X\"]\n{extra}"),
            std::path::Path::new(".coderipper.toml"),
        )
        .unwrap()
        .architecture
        .unwrap()
    };
    let stable = a("[[architecture.component]]\nname = \"p\"\nrole = \"policy\"\nvolatility = \"stable\"\n[[architecture.component]]\nname = \"q\"\nvolatility = \"stable\"\n");
    assert_eq!(
        stable
            .component
            .iter()
            .filter(|c| is_stable_policy(c))
            .count(),
        2
    );
    let none = a("[[architecture.component]]\nname = \"adapter\"\nrole = \"adapter\"\nvolatility = \"stable\"\n[[architecture.component]]\nname = \"untagged\"\nrole = \"policy\"\n");
    assert_eq!(
        none.component
            .iter()
            .filter(|c| is_stable_policy(c))
            .count(),
        0
    );
    assert!(missing_declaration(Some(&none)).unwrap().contains("stable"));
}

#[test]
fn a_missing_declaration_or_volatile_table_is_not_clean() {
    assert!(missing_declaration(None)
        .unwrap()
        .contains("no [architecture] table"));
    let a = crate::config::ConfigFile::parse(
        "[architecture]\n[[architecture.component]]\nname = \"p\"\nvolatility = \"stable\"\n",
        std::path::Path::new(".coderipper.toml"),
    )
    .unwrap()
    .architecture
    .unwrap();
    assert!(missing_declaration(Some(&a)).unwrap().contains("volatile"));
}
