/// The ways a port can be wrong.
#[derive(Debug)]
pub struct BadPort(pub String);

/// Parses the port.
pub fn port(text: &str) -> Result<u16, BadPort> {
    text.parse().map_err(|_| BadPort(text.to_string()))
}
