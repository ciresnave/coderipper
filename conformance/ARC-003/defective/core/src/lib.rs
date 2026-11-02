//! Stable policy that names a backend.
#[derive(Clone, Copy)]
pub enum BackendId {
    Cpu,
}

pub fn run(backend: BackendId) -> u32 {
    match backend {
        BackendId::Cpu => 1,
    }
}
