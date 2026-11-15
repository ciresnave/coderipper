use std::rc::Rc;
use std::sync::Arc;

async fn tick() {}

pub async fn shared(counter: Arc<u8>) -> u8 {
    tick().await;
    *counter
}

pub fn local_only(x: Rc<u8>) -> u8 {
    *x
}
