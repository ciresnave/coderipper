const SECONDS_PER_DAY: u64 = 86_400;
const REFUSED: &str = "connection refused";

pub struct Wrapper(pub u64);

pub fn is_stale(age: u64) -> bool {
    age > SECONDS_PER_DAY
}

pub fn small(x: u64) -> u64 {
    (x + 1) * 2 % 60 + 1024
}

pub fn wrapped() -> Wrapper {
    Wrapper(86_400)
}

pub fn message() -> String {
    format!("{} after {} seconds", REFUSED, 86_400)
}

pub fn twice() -> [&'static str; 2] {
    ["not found", "not found"]
}

#[cfg(test)]
mod tests {
    #[test]
    fn magic_in_tests_is_free() {
        let x = 86_400 + 37;
        let s = ["same text", "same text", "same text"];
        assert!(x > 3 && s.len() == 3);
    }
}
