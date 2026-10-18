//! The module boundary is untrusted input: a closed set (a claim's status, a request's unit and tier) is an enum, and a
//! message with a key or a value outside the protocol is rejected where it is parsed, not interpreted later.

use coderipper::check::{CheckContext, Tier};
use coderipper::module::{Hello, Limits, Request, RuleClaim};

fn hello_json(extra: &str, status: &str) -> String {
    format!(
        r#"{{"reason":"coderipper-hello","module":"m","module_version":"1","protocol":["1.0"],"languages":[],{extra}"rules":[{{"id":"X-1","status":"{status}"}}]}}"#
    )
}

#[test]
fn a_claim_status_outside_the_protocol_is_rejected_at_parse() {
    assert!(serde_json::from_str::<Hello>(&hello_json("", "implemented-native")).is_ok());
    assert!(serde_json::from_str::<Hello>(&hello_json("", "delegated")).is_ok());
    assert!(serde_json::from_str::<Hello>(&hello_json("", "not-applicable")).is_ok());
    assert!(serde_json::from_str::<Hello>(&hello_json("", "bogus")).is_err());
}

#[test]
fn a_key_the_protocol_does_not_define_is_rejected_in_a_hello_and_a_claim() {
    assert!(serde_json::from_str::<Hello>(&hello_json(r#""surprise":1,"#, "delegated")).is_err());
    let claim = r#"{"id":"X-1","status":"delegated","surprise":true}"#;
    assert!(serde_json::from_str::<RuleClaim>(claim).is_err());
}

#[test]
fn a_request_unit_or_tier_outside_the_protocol_is_rejected() {
    let request = Request::new(
        &CheckContext::new("."),
        None,
        Tier::Fast,
        vec![],
        Limits::default(),
    );
    let good = serde_json::to_value(&request).unwrap();
    assert!(
        serde_json::from_value::<Request>(good.clone()).is_ok(),
        "the control: a request the host wrote parses"
    );
    for (key, value) in [("unit", "galaxy"), ("run_tier", "never")] {
        let mut bad = good.clone();
        bad[key] = value.into();
        assert!(
            serde_json::from_value::<Request>(bad).is_err(),
            "{key} = {value}"
        );
    }
}
