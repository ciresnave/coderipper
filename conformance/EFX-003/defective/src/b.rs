pub fn b() -> Option<String> {
    std::env::var("B").ok()
}
