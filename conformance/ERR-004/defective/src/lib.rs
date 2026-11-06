/// Parses the port.
pub fn port(text: &str) -> Result<u16, String> {
    text.parse().map_err(|_| format!("bad port: {text}"))
}
