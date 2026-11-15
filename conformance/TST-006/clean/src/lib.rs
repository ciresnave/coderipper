pub fn stamp(now: u64) -> u64 { now }
#[cfg(test)]
mod tests {
    use super::stamp;
    #[test]
    fn stamps_a_given_time() {
        assert_eq!(stamp(5), 5);
    }
    #[test]
    #[ignore = "needs the real clock; run on purpose"]
    fn stamps_now() {
        let _ = std::time::SystemTime::now();
    }
    #[test]
    fn serves_locally() {
        let _ = std::net::TcpStream::connect("127.0.0.1:1");
    }
}
