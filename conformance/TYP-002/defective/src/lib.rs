pub fn rank(kind: &str) -> u8 {
    match kind {
        "low" => 1,
        "mid" => 2,
        "high" => 3,
        _ => 0,
    }
}

pub fn label(kind: &str) -> &'static str {
    if kind == "a" {
        "first"
    } else if kind == "b" {
        "second"
    } else if kind == "c" {
        "third"
    } else {
        "other"
    }
}
