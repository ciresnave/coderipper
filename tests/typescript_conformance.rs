// SPDX-License-Identifier: MIT OR Apache-2.0
//! The TypeScript module's claims are earned: each claim's proof names a fixture pair in `conformance-typescript/`, and the
//! runner finds the seeded defects there and nothing in the clean twins.

use coderipper::conformance::{run, Options, Verdict};
use coderipper::module::{Module, TypeScriptModule};
use std::path::Path;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance-typescript")
}

#[test]
fn every_typescript_claim_is_proven_by_its_fixture_pair() {
    let module = TypeScriptModule::new();
    let result = run(&module, &Options::new(fixtures())).expect("the runner runs");
    assert!(!result.rules.is_empty(), "the module claims nothing");
    for proof in &result.rules {
        assert_eq!(proof.verdict, Verdict::Proven, "{}", proof.rule);
    }
}

#[test]
fn each_claimed_proof_names_a_fixture_directory_that_exists() {
    let hello = TypeScriptModule::new().describe().unwrap();
    for claim in &hello.rules {
        let proof = claim.proof.as_deref().expect("a claim without a proof");
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(proof);
        assert!(dir.join("defective").is_dir(), "{proof}: no defective");
        assert!(dir.join("clean").is_dir(), "{proof}: no clean");
    }
}
