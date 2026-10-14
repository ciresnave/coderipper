pub fn used() -> i32 {
    1
}

/// A scoped helper: `T` is whatever the closure returns, so a caller whose closure returns nothing discards nothing.
pub fn with<T>(f: impl FnOnce() -> T) -> T {
    f()
}
