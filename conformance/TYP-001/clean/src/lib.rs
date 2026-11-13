/// A user, not a number.
pub struct UserId(pub u64);

/// An amount of money in the smallest unit, not a float.
pub struct Money(pub i64);

pub fn charge(user: UserId, amount: Money) -> bool {
    user.0 > 0 && amount.0 > 0
}

/// The unit is in the name.
pub fn wait(timeout_ms: u64) -> u64 {
    timeout_ms
}
