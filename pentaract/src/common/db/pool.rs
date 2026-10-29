
use std::time::Duration;

use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn get_pool(
    dsn: &str,
    max_connection: u32,
    timeout: Duration,
) -> PgPool {
    let max_connections = max_connection.clamp(1, 2);

    let db = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(timeout)
        .connect(dsn)
        .await
        .expect("can't establish database connection");

    tracing::info!(
        "Database pool established; max connections: {}",
        max_connections
    );

    db
}
