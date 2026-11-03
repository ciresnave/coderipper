pub fn plus(a: u8, b: u8) -> u8 {
    crate::add(a, b).saturating_add(1)
}
