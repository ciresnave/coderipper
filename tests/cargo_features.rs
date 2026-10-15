#[allow(dead_code)] // shared helpers; this file uses only some of them
mod common;

use coderipper::check::{CargoFeatures, Check, CheckContext};
use coderipper::checks::UnusedParametersCheck;
use common::git_repo_with;

const GATED_MANIFEST: &str = "[package]
name = \"fixture\"
version = \"0.1.0\"
edition = \"2021\"

[features]
capture = []

[[bin]]
name = \"capture\"
path = \"src/bin/capture.rs\"
required-features = [\"capture\"]
";

fn subjects(findings: &[coderipper::finding::Finding]) -> Vec<String> {
    let mut v: Vec<String> = findings.iter().filter_map(|f| f.subject.clone()).collect();
    v.sort();
    v
}

#[test]
fn enabling_the_required_feature_builds_the_gated_bin_and_checks_it() {
    let repo = git_repo_with(&[
        ("Cargo.toml", GATED_MANIFEST),
        ("src/lib.rs", "pub fn f(a: i32, b: i32) -> i32 { a }\n"),
        (
            "src/bin/capture.rs",
            "fn main() { h(1); }\nfn h(q: i32) {}\n",
        ),
    ]);
    let off = UnusedParametersCheck::new()
        .run(&CheckContext::new(repo.path().to_path_buf()))
        .unwrap();
    assert_eq!(
        subjects(&off),
        vec!["f::b"],
        "the control: the bin is not built without the feature"
    );

    for features in [
        CargoFeatures::default().with_features(["capture"]),
        CargoFeatures::default().all_features(),
    ] {
        let ctx = CheckContext::new(repo.path().to_path_buf()).cargo_features(features.clone());
        let on = UnusedParametersCheck::new().run(&ctx).unwrap();
        assert_eq!(subjects(&on), vec!["f::b", "h::q"], "{features:?}");
        let control = on[0].positive_control.as_deref().unwrap();
        assert!(!control.contains("target not built"), "{control}");
    }
}
