use crate::shape::Shape;

pub fn corners(shape: &Shape) -> u32 {
    match shape {
        Shape::Circle(_) => 0,
        _ => 4,
    }
}
