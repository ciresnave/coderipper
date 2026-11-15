pub trait Shape {
    fn is_round(&self) -> bool;
}

pub fn is_round(s: &dyn Shape) -> bool {
    s.is_round()
}

pub struct Circle;

impl Shape for Circle {
    fn is_round(&self) -> bool {
        true
    }
}
