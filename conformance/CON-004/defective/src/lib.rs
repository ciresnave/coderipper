use std::sync::mpsc;

pub fn start() -> mpsc::Sender<String> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in rx {
            println!("{line}");
        }
    });
    tx
}
