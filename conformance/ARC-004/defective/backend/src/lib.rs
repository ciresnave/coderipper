//! The volatile adapter: it may name its own database types.
use sqlx::PgPool;

pub fn open(pool: &PgPool) -> u32 {
    let _ = pool;
    1
}
