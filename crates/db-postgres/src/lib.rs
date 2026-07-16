pub use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::{PgPool, postgres::PgPoolOptions};

// Add repo implementation modules here, e.g.:
// pub mod account;

pub fn get_connection_pool(options: PgConnectOptions) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(options)
}
