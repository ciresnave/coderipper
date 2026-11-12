use std::cell::Cell;
use std::sync::{Arc, Mutex};

pub struct Cache {
    /// Owned by the render loop; read by nobody else, reset on each frame.
    pub hits: Cell<u32>,
    // Written only by the writer thread under the lock; cleared when the file is reloaded.
    pub rows: Arc<Mutex<Vec<String>>>,
    pub name: String,
}
