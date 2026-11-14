pub fn parse(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
}

pub fn is_word(text: &str) -> bool {
    regex::Regex::new("^[a-z]+$").map(|r| r.is_match(text)).unwrap_or(false)
}
