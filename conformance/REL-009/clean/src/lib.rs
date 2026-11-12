use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub fn timed() -> u128 {
    let start = Instant::now();
    work();
    start.elapsed().as_millis()
}

/// A timestamp to store, not an interval.
pub fn stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn work() {}
