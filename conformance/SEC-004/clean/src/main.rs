fn main() {
    let secret = std::env::var("SIGNING_KEY").unwrap_or_default();
    let key = EncodingKey::from_secret(secret.as_bytes());
    let client = Client::builder()
        .danger_accept_invalid_certs(false)
        .build();
    let _ = (key, client);
}

#[cfg(test)]
mod tests {
    #[test]
    fn signs() {
        let _ = EncodingKey::from_secret(b"test-only-key");
    }
}
