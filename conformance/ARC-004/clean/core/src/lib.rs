//! Policy that owns its boundary: the signature names a trait of its own, and no infrastructure.
pub trait Store {
    fn get(&self, id: u32) -> u32;
}

pub fn load(store: &dyn Store, id: u32) -> u32 {
    store.get(id)
}
