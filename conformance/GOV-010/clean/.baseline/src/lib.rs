// The same shapes, each saying why (the commit before this one, with the same suppressions).
// #[allow(dead_code)]

#[allow(dead_code, reason = "called only by the linker")]
fn kept() {}

#[test]
#[ignore = "reads a live web page; the scheduled job runs it"]
fn slow() {}

const TEXT: &str = "#[allow(dead_code)] and #[ignore]";
