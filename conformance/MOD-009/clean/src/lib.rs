pub struct Root { b: B }
pub struct B { c: C }
pub struct C { d: D }
pub struct D { e: E }
pub struct E { n: u32 }

impl Root { pub fn n(&self) -> u32 { self.b.n() } }
impl B { pub fn n(&self) -> u32 { self.c.n() } }
impl C { pub fn n(&self) -> u32 { self.d.n() } }
impl D { pub fn n(&self) -> u32 { self.e.n() } }
impl E { pub fn n(&self) -> u32 { self.n } }

pub fn depth(r: &Root) -> u32 {
    r.n()
}
