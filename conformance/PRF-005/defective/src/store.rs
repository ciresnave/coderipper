pub struct Store {
    pub items: Vec<u8>,
    pub response_cache: Vec<String>,
}

impl Store {
    pub fn add(&mut self, s: String) {
        self.response_cache.push(s);
        self.items.push(1);
    }
}
