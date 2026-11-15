/// The library's own answer type; `serde_json` stays an implementation detail.
pub struct Document {
    text: String,
}

impl Document {
    pub fn text(&self) -> &str {
        &self.text
    }
}

pub fn parse(text: &str) -> Document {
    let value: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    Document {
        text: value.to_string(),
    }
}
