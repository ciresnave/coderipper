use std::sync::mpsc;

pub fn start() -> mpsc::SyncSender<String> {
    let (tx, rx) = mpsc::sync_channel::<String>(64);
    std::thread::spawn(move || {
        for line in rx {
            println!("{line}");
        }
    });
    tx
}
