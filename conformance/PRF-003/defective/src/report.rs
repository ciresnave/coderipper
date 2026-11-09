pub fn join(parts: &[&str]) -> String {
    let mut s = String::new();
    for p in parts {
        s = format!("{}{}", s, p);
    }
    s
}
