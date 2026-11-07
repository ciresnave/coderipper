// Seeded defect: a dead_code suppression and an ignored test, neither with a reason.

#[allow(dead_code)]
fn unused() {}

#[test]
#[ignore]
fn slow() {}
