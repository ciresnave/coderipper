use std::collections::HashMap;

pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
    fn delete(&mut self, key: &str);

    /// A default body is not a stub: the trait says what the absence means.
    fn flush(&mut self) {
        todo!("subclasses decide")
    }
}

pub struct Memory(HashMap<String, String>);

impl Store for Memory {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn delete(&mut self, key: &str) {
        self.0.remove(key);
    }

    fn flush(&mut self) {}
}
