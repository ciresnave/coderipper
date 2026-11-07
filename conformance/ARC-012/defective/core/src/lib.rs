//! Policy that builds its own adapter.
use store::DiskStore;

pub trait Store {
    fn get(&self, id: u32) -> u32;
}

pub fn load(store: &dyn Store, id: u32) -> u32 {
    store.get(id)
}

pub fn load_default(id: u32) -> u32 {
    let s = DiskStore::new(7);
    load(&s, id)
}
