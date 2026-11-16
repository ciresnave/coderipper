//! A counter whose `bump` changes the count and also says what it is now.
pub struct Counter(u32);

impl Counter {
    pub fn bump(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}
