//! Policy whose public error type carries a driver error and an HTTP status code.
pub enum OrderError {
    NotFound(u64),
    Db(sqlx::Error),
}

pub struct ApiError {
    pub status: http::StatusCode,
    pub message: String,
}
