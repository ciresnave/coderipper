use std::rc::Rc;

async fn tick() {}

pub async fn shared(counter: Rc<u8>) -> u8 {
    tick().await;
    *counter
}
