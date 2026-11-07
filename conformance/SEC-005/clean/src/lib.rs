/// Reads one byte through a raw pointer.
///
/// # Safety
/// `p` must be valid for reads.
pub unsafe fn first_byte_raw(p: *const u8) -> u8 {
    // SAFETY: the caller promises `p` is valid for reads.
    unsafe { *p }
}

pub fn first_of(slice: &[u8]) -> u8 {
    // SAFETY: a slice's pointer is valid for reads of its first element when it has one.
    unsafe { *slice.as_ptr() }
}
