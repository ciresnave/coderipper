/// Logs a login without the secret.
pub fn login(user: &str, password: &str) {
    log::info!("login of {} ({} byte password)", user, password.len());
}
