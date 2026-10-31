pub fn a() -> Option<String> {
    std::env::var("A").ok()
}
