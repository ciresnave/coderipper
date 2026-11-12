use std::fmt;

/// A value that prints as a mask.
pub struct Secret<T>(T);

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug)]
pub struct Login {
    pub user: String,
    pub password: Secret<String>,
    pub token_type: String,
}

pub struct Session {
    pub token: String,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session").field("token", &"[REDACTED]").finish()
    }
}
