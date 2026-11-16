// The same shapes, each saying why; and text that only looks like a suppression.
// #[allow(dead_code)]

#[allow(dead_code, reason = "called only by the linker")]
fn kept() {}

#[test]
#[ignore = "reads a live web page; the scheduled job runs it"]
fn slow() {}

const TEXT: &str = "#[allow(dead_code)] and #[ignore]";
