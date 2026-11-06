use std::time::Duration;

pub async fn poll_until_ready() {
    loop {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn ready() -> bool {
    true
}
