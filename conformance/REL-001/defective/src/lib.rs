/// The client every outbound call of the service goes through.
pub fn client() -> reqwest::Client {
    reqwest::Client::new()
}
