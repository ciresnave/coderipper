use std::sync::atomic::{AtomicUsize, Ordering};

pub fn next(counter: &AtomicUsize) -> usize {
    // Relaxed: a plain ticket counter, no other data is published through it.
    counter.fetch_add(1, Ordering::Relaxed)
}
