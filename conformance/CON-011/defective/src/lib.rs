use std::sync::Mutex;

/// A registry any code can push into.
pub static REGISTRY: Mutex<Vec<String>> = Mutex::new(Vec::new());
