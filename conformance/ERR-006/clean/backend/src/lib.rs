//! The adapter: it names the driver error and turns it into a domain error at the edge.
pub enum StoreError {
    Db(sqlx::Error),
}

pub fn translate(e: StoreError) -> u32 {
    match e {
        StoreError::Db(_) => 1,
    }
}
