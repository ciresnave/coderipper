/// The client every outbound call of the service goes through: it gives up after five seconds.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}
