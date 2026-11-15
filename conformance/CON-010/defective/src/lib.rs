use std::sync::atomic::{AtomicUsize, Ordering};

pub fn next(counter: &AtomicUsize) -> usize {
    counter.fetch_add(1, Ordering::Relaxed)
}
