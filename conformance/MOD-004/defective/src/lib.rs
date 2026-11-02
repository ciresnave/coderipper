pub struct Mixed {
    log: Vec<String>,
    level: u8,
    cache: Vec<u8>,
    hits: u32,
}

impl Mixed {
    pub fn record(&mut self, line: &str) {
        self.log.push(line.to_string());
        self.level += 1;
        self.log.truncate(100);
    }

    pub fn summary(&self) -> String {
        let n = self.log.len();
        let l = self.level;
        format!("{n} lines at level {l}")
    }

    pub fn lookup(&mut self, key: u8) -> bool {
        let found = self.cache.contains(&key);
        self.hits += 1;
        found
    }

    pub fn store(&mut self, key: u8) {
        self.cache.push(key);
        self.hits += 1;
        self.cache.dedup();
    }
}
