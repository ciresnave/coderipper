// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;

fn site(file: &str, line: usize) -> Site {
    Site {
        file: file.into(),
        line,
    }
}

fn edges(list: &[(&str, &str)]) -> BTreeMap<(String, String), Site> {
    list.iter()
        .enumerate()
        .map(|(i, (a, b))| ((a.to_string(), b.to_string()), site("src/x.rs", i + 1)))
        .collect()
}

fn commit(id: &str, files: &[&str]) -> Commit {
    Commit {
        id: id.into(),
        files: files.iter().map(|s| s.to_string()).collect(),
    }
}

fn files() -> BTreeMap<String, BTreeSet<String>> {
    let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
    BTreeMap::from([
        ("a".to_string(), set(&["src/a.rs", "src/a/inner.rs"])),
        ("b".to_string(), set(&["src/b.rs"])),
    ])
}

fn pairs() -> BTreeMap<(String, String), (Site, Site)> {
    mutual_pairs(&edges(&[("a", "b"), ("b", "a")]))
}

#[test]
fn only_a_reference_in_both_directions_makes_a_pair() {
    assert!(mutual_pairs(&edges(&[("a", "b")])).is_empty(), "one way");
    assert!(
        mutual_pairs(&edges(&[("a", "b"), ("c", "a")])).is_empty(),
        "a chain"
    );
    let p = mutual_pairs(&edges(&[("a", "b"), ("b", "a")]));
    assert_eq!(p.len(), 1);
    let ((a, b), (forward, back)) = p.iter().next().unwrap();
    assert_eq!((a.as_str(), b.as_str()), ("a", "b"));
    assert_eq!((forward.line, back.line), (1, 2));
}

#[test]
fn references_between_submodules_count_for_their_top_level_modules() {
    let p = mutual_pairs(&edges(&[("a::x", "b::y"), ("b", "a")]));
    assert_eq!(
        p.keys().collect::<Vec<_>>(),
        [&("a".to_string(), "b".to_string())]
    );
    // references inside one top-level module are no pair
    assert!(mutual_pairs(&edges(&[("a::x", "a::y"), ("a::y", "a::x")])).is_empty());
}

#[test]
fn modules_changed_together_in_enough_commits_and_share_are_entangled() {
    let commits = [
        commit("c1", &["src/a.rs", "src/b.rs"]),
        commit("c2", &["src/a/inner.rs", "src/b.rs"]),
        commit("c3", &["src/a.rs", "src/b.rs"]),
        commit("c4", &["src/a.rs"]),
        commit("c5", &["src/other.rs"]),
    ];
    let (found, tally) = analyse(&pairs(), &files(), &commits, Params::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].both, found[0].either), (3, 4));
    assert_eq!(found[0].sample, ["c1", "c2", "c3"]);
    // positive control: the pair was judged and the history was read
    assert_eq!((tally.read, tally.bulk, tally.pairs), (5, 0, 1));
}

#[test]
fn a_pair_edited_apart_or_too_rarely_together_is_silent() {
    let apart = [
        commit("c1", &["src/a.rs", "src/b.rs"]),
        commit("c2", &["src/a.rs"]),
        commit("c3", &["src/b.rs"]),
        commit("c4", &["src/a.rs"]),
        commit("c5", &["src/b.rs"]),
    ];
    let (found, tally) = analyse(&pairs(), &files(), &apart, Params::default());
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(tally.pairs, 1, "the pair was judged");
    // enough commits together but a small share of everything that touched either
    let mut diluted: Vec<Commit> = (0..3)
        .map(|i| commit(&format!("t{i}"), &["src/a.rs", "src/b.rs"]))
        .collect();
    diluted.extend((0..5).map(|i| commit(&format!("a{i}"), &["src/a.rs"])));
    let (found, _) = analyse(&pairs(), &files(), &diluted, Params::default());
    assert!(found.is_empty(), "3 of 8 is 37%: {found:?}");
    let lenient = Params {
        min_share_percent: 30,
        ..Params::default()
    };
    let (found, _) = analyse(&pairs(), &files(), &diluted, lenient);
    assert_eq!(found.len(), 1, "positive control: the same history at 30%");
}

#[test]
fn a_bulk_commit_is_set_aside() {
    let many: Vec<String> = (0..30).map(|i| format!("src/f{i}.rs")).collect();
    let mut bulk: Vec<&str> = many.iter().map(String::as_str).collect();
    bulk.extend(["src/a.rs", "src/b.rs"]);
    let commits = [
        commit("c1", &["src/a.rs", "src/b.rs"]),
        commit("c2", &["src/a.rs", "src/b.rs"]),
        commit("bulk", &bulk),
    ];
    let (found, tally) = analyse(&pairs(), &files(), &commits, Params::default());
    assert_eq!(tally.bulk, 1);
    assert!(
        found.is_empty(),
        "two commits are below the minimum: {found:?}"
    );
    let kept = Params {
        max_files: 100,
        ..Params::default()
    };
    let (found, _) = analyse(&pairs(), &files(), &commits, kept);
    assert_eq!(found.len(), 1, "positive control: kept, it is the third");
}

#[test]
fn the_finding_names_both_edges_and_is_info_at_low_confidence() {
    let commits: Vec<Commit> = (0..3)
        .map(|i| {
            commit(
                &format!("{i}{i}{i}{i}{i}{i}{i}{i}{i}"),
                &["src/a.rs", "src/b.rs"],
            )
        })
        .collect();
    let (found, _) = analyse(&pairs(), &files(), &commits, Params::default());
    let f = judge("p", "app", "crates/app", &found[0], Params::default());
    assert_eq!(f.severity, Severity::Info);
    assert_eq!(f.confidence, Confidence::Low);
    assert_eq!(f.subject.as_deref(), Some("app:a,b"));
    assert!(f
        .location
        .as_ref()
        .is_some_and(|l| l.file == "crates/app/src/x.rs"));
    // the host rejects a summary that reads as an absence claim without a positive control
    assert!(f.clone().validate().is_ok(), "{}", f.summary);
    assert!(f.detail.contains("00000000"), "{}", f.detail);
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &std::path::Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A crate whose modules `a` and `b` refer to each other, then three more commits that edit both in one commit each, or alternately.
fn repo(together: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git_in(p, &["init", "-q"]);
    write(
        p,
        "Cargo.toml",
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(p, "src/lib.rs", "pub mod a;\npub mod b;\n");
    write(
        p,
        "src/a.rs",
        "pub fn f() -> u32 { crate::b::g() }\npub fn h() -> u32 { 1 }\n",
    );
    write(p, "src/b.rs", "pub fn g() -> u32 { crate::a::h() }\n");
    git_in(p, &["add", "-A"]);
    git_in(p, &["commit", "-q", "-m", "one"]);
    for i in 0..3 {
        let note_a = format!("// a edit {i}\n");
        let note_b = format!("// b edit {i}\n");
        let touch = |rel: &str, note: &str| {
            let path = p.join(rel);
            let mut text = std::fs::read_to_string(&path).unwrap();
            text.push_str(note);
            std::fs::write(path, text).unwrap();
        };
        if together {
            touch("src/a.rs", &note_a);
            touch("src/b.rs", &note_b);
        } else if i % 2 == 0 {
            touch("src/a.rs", &note_a);
        } else {
            touch("src/b.rs", &note_b);
        }
        git_in(p, &["add", "-A"]);
        git_in(p, &["commit", "-q", "-m", &format!("edit {i}")]);
    }
    dir
}

#[test]
fn a_real_repository_reports_the_pair_edited_together_and_not_the_one_edited_apart() {
    let dir = repo(true);
    let (f, notes) = EntangledCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(f[0].subject.as_deref(), Some("app:a,b"));
    assert!(
        f[0].summary.contains("4 of the 4 commits"),
        "{}",
        f[0].summary
    );
    assert!(f[0].clone().validate().is_ok());
    assert!(notes.iter().any(|n| n.contains("1 pair(s)")), "{notes:?}");

    let apart = repo(false);
    let (f, notes) = EntangledCheck::new()
        .run_noted(&CheckContext::new(apart.path()))
        .unwrap();
    assert!(f.is_empty(), "{f:?}");
    // positive control: the same modules refer to each other and were compared
    assert!(notes.iter().any(|n| n.contains("1 pair(s)")), "{notes:?}");
}

#[test]
fn settings_move_the_thresholds() {
    let dir = repo(true);
    write(
        dir.path(),
        ".coderipper.toml",
        "[arc011]\nmin_commits = 9\n",
    );
    let (f, _) = EntangledCheck::new()
        .run_noted(&CheckContext::new(dir.path()))
        .unwrap();
    assert!(f.is_empty(), "four commits are below nine: {f:?}");
}
