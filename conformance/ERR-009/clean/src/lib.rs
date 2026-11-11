/// Rejects a bad login without repeating the secret.
pub fn reject(token: &str) -> anyhow::Result<()> {
    anyhow::bail!("rejected a token of {} bytes", token.len())
}
