/// Reads one byte through a raw pointer.
pub fn first_byte(p: *const u8) -> u8 {
    unsafe { *p }
}
