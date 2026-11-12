pub fn charge(user_id: u64, amount: f64) -> bool {
    user_id > 0 && amount > 0.0
}
