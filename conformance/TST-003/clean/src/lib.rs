pub fn add(a: u8, b: u8) -> u8 { a + b }
#[cfg(test)]
mod tests {
    use super::add;
    #[test]
    fn adds() { assert_eq!(add(1, 2), 3); }
    #[test]
    fn smoke() { add(1, 1); }
    #[test]
    #[ignore = "placeholder until the carry case is specified"]
    fn carries() {}
}
