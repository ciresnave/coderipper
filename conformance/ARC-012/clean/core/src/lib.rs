//! Policy that takes its adapter from whoever wires the program.
pub trait Store {
    fn get(&self, id: u32) -> u32;
}

pub fn load(store: &dyn Store, id: u32) -> u32 {
    store.get(id)
}
