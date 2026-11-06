//! Policy whose public function takes a database handle.
use sqlx::PgPool;

pub fn load(pool: &PgPool, id: u32) -> u32 {
    let _ = pool;
    id
}
