//! The same component with the effects moved to the caller: the rate and the time come in as arguments.
use std::time::Duration;

pub fn price(base: f64, rate: f64) -> f64 {
    base * rate
}

pub fn age(now_secs: u64, then_secs: u64) -> Duration {
    Duration::from_secs(now_secs.saturating_sub(then_secs))
}
