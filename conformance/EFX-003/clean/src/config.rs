pub fn home() -> Option<String> {
    std::env::var("HOME").ok()
}
