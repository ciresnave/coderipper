pub struct Store {
    pub items: Vec<u8>,
    pub response_cache: Vec<String>,
}

impl Store {
    pub fn add(&mut self, s: String) {
        if self.response_cache.len() >= 256 {
            self.response_cache.remove(0);
        }
        self.response_cache.push(s);
        self.items.push(1);
    }
}
