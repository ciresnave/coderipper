use std::cell::Cell;
use std::sync::{Arc, Mutex};

pub struct Cache {
    pub hits: Cell<u32>,
    pub rows: Arc<Mutex<Vec<String>>>,
}
