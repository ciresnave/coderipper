use std::time::Duration;

pub fn connect_forever(open: fn() -> Result<u8, ()>) -> u8 {
    loop {
        match open() {
            Ok(c) => return c,
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
