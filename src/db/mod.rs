pub mod entities;

use std::time::Duration;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub async fn init_db(url: &str, max_connections: u32) -> DatabaseConnection {
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(max_connections)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(5))
        .acquire_timeout(Duration::from_secs(60))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .sqlx_logging(false);
    Database::connect(opt).await
        .expect("Failed to connect to database")
}