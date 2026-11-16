pub fn port(text: &str) -> Result<u16, std::num::ParseIntError> {
    text.parse::<u16>()
}

pub fn retries(text: &str) -> u32 {
    // the setting is optional: fall back to three attempts
    text.parse::<u32>().unwrap_or(3)
}
