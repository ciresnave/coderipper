use std::time::Duration;

pub async fn poll_until_ready() {
    loop {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn ready() -> bool {
    true
}
