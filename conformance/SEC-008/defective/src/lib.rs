/// Logs a login.
pub fn login(user: &str, password: &str) {
    log::info!("login of {} with {}", user, password);
}
