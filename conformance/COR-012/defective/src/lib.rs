use std::rc::Rc;

pub struct Shared {
    pub count: Rc<u8>,
}

unsafe impl Send for Shared {}
