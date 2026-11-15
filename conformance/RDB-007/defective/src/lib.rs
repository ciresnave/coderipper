pub mod names;

pub fn is_stale(age: u64) -> bool {
    age > 86_400
}
