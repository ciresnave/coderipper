//! A component declared pure that reads the environment and the clock.
use std::time::SystemTime;

pub fn rate() -> f64 {
    std::env::var("RATE").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0)
}

pub fn stamp() -> SystemTime {
    SystemTime::now()
}
