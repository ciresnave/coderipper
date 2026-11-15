use std::sync::Mutex;

/// The registry is owned by the caller and passed in.
pub struct Registry(pub Mutex<Vec<String>>);

pub static NAME: &str = "service";
