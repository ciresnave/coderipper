#[derive(Debug)]
pub struct Login {
    pub user: String,
    pub password: String,
}

pub struct Session {
    pub token: String,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session({})", self.token)
    }
}
