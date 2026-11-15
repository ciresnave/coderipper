use std::fmt::Write;

pub fn join(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        s.push_str(p);
        let _ = write!(s, "-");
    }
    s
}
