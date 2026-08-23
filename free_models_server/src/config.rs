use std::env;
use std::time::Duration;

pub struct Config {
    pub server_host: String,
    pub server_port: u16,
    pub scheduler_cache_ttl: Duration,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_open_ttl: Duration,
    pub encryption_key: [u8; 32],
    pub cache_affinity_max_capacity: u64,
    pub cache_affinity_ttl: Duration,
    pub api_key_cache_max_capacity: u64,
    pub circuit_breaker_max_capacity: u64,
    pub ssrf_cache_max_capacity: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            server_host: env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("SERVER_PORT must be a valid u16"),
            scheduler_cache_ttl: Duration::from_secs(
                env::var("SCHEDULER_CACHE_TTL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            circuit_breaker_threshold: env::var("CIRCUIT_BREAKER_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            circuit_breaker_open_ttl: Duration::from_secs(
                env::var("CIRCUIT_BREAKER_OPEN_TTL")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            encryption_key: crate::util::encryption::parse_key(
                &env::var("ENCRYPTION_KEY")
                    .expect("ENCRYPTION_KEY must be set (64 hex chars = 32 bytes)"),
            )
            .expect("ENCRYPTION_KEY must be valid hex (64 chars = 32 bytes)"),
            cache_affinity_max_capacity: env::var("CACHE_AFFINITY_MAX_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            cache_affinity_ttl: Duration::from_secs(
                env::var("CACHE_AFFINITY_TTL_SEC")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            api_key_cache_max_capacity: env::var("API_KEY_CACHE_MAX_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            circuit_breaker_max_capacity: env::var("CIRCUIT_BREAKER_MAX_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            ssrf_cache_max_capacity: env::var("SSRF_CACHE_MAX_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
        }
    }
}
