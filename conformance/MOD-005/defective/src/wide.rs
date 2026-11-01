pub struct Wide {
    pub a00: u32,
    pub a01: u32,
    pub a02: u32,
    pub a03: u32,
    pub a04: u32,
    pub a05: u32,
    pub a06: u32,
    pub a07: u32,
    pub a08: u32,
    pub a09: u32,
    pub a10: u32,
    pub a11: u32,
    pub a12: u32,
    pub a13: u32,
    pub a14: u32,
    pub a15: u32,
    pub a16: u32,
    pub a17: u32,
    pub a18: u32,
    pub a19: u32,
    pub a20: u32,
    pub a21: u32,
    pub a22: u32,
    pub a23: u32,
    pub a24: u32,
    pub a25: u32,
    pub a26: u32,
    pub a27: u32,
    pub a28: u32,
    pub a29: u32,
}

impl Wide {
    pub fn first(&self) -> u32 {
        self.a00
    }
}
