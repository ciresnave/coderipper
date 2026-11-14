//! Policy whose public errors are in domain terms; the adapter translates driver and HTTP errors into them.
pub enum OrderError {
    NotFound(u64),
    Unavailable,
}

pub struct ApiError {
    pub kind: String,
    pub message: String,
}
