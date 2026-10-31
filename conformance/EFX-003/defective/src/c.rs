pub fn c() -> Option<String> {
    std::env::var("C").ok()
}
