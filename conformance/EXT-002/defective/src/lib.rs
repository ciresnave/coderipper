use std::any::Any;

pub trait Shape {
    fn as_any(&self) -> &dyn Any;
}

pub fn is_circle(s: &dyn Shape) -> bool {
    s.as_any().downcast_ref::<Circle>().is_some()
}

pub struct Circle;
