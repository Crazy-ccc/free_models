use std::env;
use std::time::Duration;

pub struct Config {
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub db_max_connections: u32,
    pub redis_url: String,
    pub redis_enabled: bool,
    pub redis_cache_ttl_model: Duration,
    pub redis_cache_ttl_provider: Duration,
    pub redis_cache_ttl_penalty: Duration,
    pub encryption_key: [u8; 32],
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
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
            redis_enabled: env::var("REDIS_ENABLED")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            redis_cache_ttl_model: Duration::from_secs(
                env::var("REDIS_CACHE_TTL_MODEL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            redis_cache_ttl_provider: Duration::from_secs(
                env::var("REDIS_CACHE_TTL_PROVIDER_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600),
            ),
            redis_cache_ttl_penalty: Duration::from_secs(
                env::var("REDIS_CACHE_TTL_PENALTY_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1800),
            ),
            encryption_key: crate::util::encryption::parse_key(
                &env::var("ENCRYPTION_KEY")
                    .expect("ENCRYPTION_KEY must be set (64 hex chars = 32 bytes)"),
            )
            .expect("ENCRYPTION_KEY must be valid hex (64 chars = 32 bytes)"),
        }
    }
}
