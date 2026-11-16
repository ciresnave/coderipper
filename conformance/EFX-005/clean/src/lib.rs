//! A counter whose command and query are separate calls.
pub struct Counter(u32);

impl Counter {
    pub fn bump(&mut self) {
        self.0 += 1;
    }

    pub fn value(&self) -> u32 {
        self.0
    }

    /// An accepted idiom: it hands back what it took.
    pub fn take(&mut self) -> u32 {
        std::mem::take(&mut self.0)
    }
}
