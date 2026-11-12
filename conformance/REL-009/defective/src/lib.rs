use std::time::SystemTime;

pub fn timed() -> u128 {
    let start = SystemTime::now();
    work();
    start.elapsed().map(|d| d.as_millis()).unwrap_or(0)
}

fn work() {}
