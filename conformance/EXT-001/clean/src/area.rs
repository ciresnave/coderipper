use crate::shape::Shape;

pub fn corners(shape: &Shape) -> u32 {
    match shape {
        Shape::Circle(_) => 0,
        Shape::Square(_) => 4,
        Shape::Triangle(..) => 3,
    }
}
