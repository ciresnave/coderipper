//! Stable policy that names no backend: it takes a boundary it owns.
pub trait Executor {
    fn run(&self) -> u32;
}

pub fn run(executor: &dyn Executor) -> u32 {
    executor.run()
}
