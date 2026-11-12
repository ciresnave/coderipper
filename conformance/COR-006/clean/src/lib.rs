pub fn read(p: *const u8) -> u8 {
    // SAFETY: the caller passes a pointer to a live, initialised byte; see the `# Safety` section below.
    unsafe { *p }
}

/// # Safety
/// `p` must point to a live, initialised byte.
pub unsafe fn read_raw(p: *const u8) -> u8 {
    // SAFETY: the contract above is the caller's.
    unsafe { *p }
}
