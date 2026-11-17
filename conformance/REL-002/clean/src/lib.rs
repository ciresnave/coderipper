use std::time::Duration;

pub fn connect_capped(open: fn() -> Result<u8, ()>) -> Option<u8> {
    let mut delay = Duration::from_millis(100);
    for attempt in 0..5 {
        match open() {
            Ok(c) => return Some(c),
            Err(_) if attempt < 4 => {
                std::thread::sleep(delay);
                delay *= 2;
            }
            Err(_) => {}
        }
    }
    None
}
