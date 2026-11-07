//! The adapter: a concrete store that implements the port core declares.
pub struct DiskStore {
    pub root: u32,
}

impl DiskStore {
    pub fn new(root: u32) -> Self {
        Self { root }
    }
}

impl core::Store for DiskStore {
    fn get(&self, id: u32) -> u32 {
        self.root + id
    }
}
