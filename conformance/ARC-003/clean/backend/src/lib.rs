//! The volatile adapter: it may name its own backend.
#[derive(Clone, Copy)]
pub enum BackendId {
    Cpu,
}

pub fn default_backend() -> BackendId {
    BackendId::Cpu
}
