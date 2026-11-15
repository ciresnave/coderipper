/// Rejects a bad login.
pub fn reject(token: &str) -> anyhow::Result<()> {
    anyhow::bail!("rejected token {}", token)
}
