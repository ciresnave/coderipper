fn flush_metrics() {}

pub fn start() {
    std::thread::spawn(flush_metrics);
}
