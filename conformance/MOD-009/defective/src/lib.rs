pub struct Root { pub b: B }
pub struct B { pub c: C }
pub struct C { pub d: D }
pub struct D { pub e: E }
pub struct E { pub n: u32 }

pub fn depth(r: &Root) -> u32 {
    r.b.c.d.e.n
}
