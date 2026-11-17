pub fn parse_port(text: &str) -> Option<u16> {
    std::panic::catch_unwind(|| text.parse().unwrap()).ok()
}
