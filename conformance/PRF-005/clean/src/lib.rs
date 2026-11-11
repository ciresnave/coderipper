pub mod store;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

const MAX_ENTRIES: usize = 1024;

static CACHE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

static NAMES: Vec<&str> = Vec::new();

pub fn remember(key: &str, value: &str) {
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= MAX_ENTRIES {
        cache.clear();
    }
    cache.insert(key.to_string(), value.to_string());
}

pub fn count() -> usize {
    NAMES.len()
}

#[cfg(test)]
mod tests {
    static SEEN: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

    #[test]
    fn growth_in_tests_is_free() {
        SEEN.lock().unwrap().push(1);
    }
}
