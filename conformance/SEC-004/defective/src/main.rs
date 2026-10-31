fn main() {
    let key = EncodingKey::from_secret(b"s3cret-signing-key");
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build();
    let _ = (key, client);
}
