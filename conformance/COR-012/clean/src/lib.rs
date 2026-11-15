use std::sync::Arc;

pub struct Shared {
    pub count: Arc<u8>,
}

pub struct Wrapper<T> {
    pub value: T,
}

// SAFETY: the value is only reached through the owner, which requires T: Send
unsafe impl<T: Send> Send for Wrapper<T> {}
