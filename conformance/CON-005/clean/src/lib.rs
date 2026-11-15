fn flush_metrics() {}

pub fn start() -> std::thread::JoinHandle<()> {
    std::thread::spawn(flush_metrics)
}

pub fn run_and_wait() {
    let handle = std::thread::spawn(flush_metrics);
    handle.join().unwrap();
}

pub fn heartbeat() {
    // fire-and-forget: the heartbeat owns itself and stops with the process
    std::thread::spawn(flush_metrics);
}
