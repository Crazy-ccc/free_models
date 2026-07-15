use std::env;
use std::time::Duration;

pub struct Config {
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub db_max_connections: u32,
    pub model_cache_ttl: Duration,
    pub provider_cache_ttl: Duration,
    pub penalty_ttl: Duration,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set"),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            server_host: env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("SERVER_PORT must be a valid u16"),
            model_cache_ttl: Duration::from_secs(
                env::var("MODEL_CACHE_TTL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            provider_cache_ttl: Duration::from_secs(
                env::var("PROVIDER_CACHE_TTL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600),
            ),
            penalty_ttl: Duration::from_secs(
                env::var("PENALTY_TTL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1800),
            ),
        }
    }
}