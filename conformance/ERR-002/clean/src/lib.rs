pub fn parse_port(text: &str) -> Option<u16> {
    text.parse().ok()
}

pub fn run_worker(job: fn()) -> bool {
    // the outermost function of the worker thread: a panic in a job must not take the pool down
    std::panic::catch_unwind(job).is_ok()
}
