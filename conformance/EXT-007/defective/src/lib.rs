trait Store {
    fn get(&self, key: &str) -> Option<String>;
}

struct Memory;

impl Store for Memory {
    fn get(&self, _key: &str) -> Option<String> {
        None
    }
}

pub fn lookup(key: &str) -> Option<String> {
    Memory.get(key)
}
