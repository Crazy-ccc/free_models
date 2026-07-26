use std::env;
use log::warn;
use redis::AsyncCommands;

#[derive(Clone)]
pub struct RedisManager {
    client: Option<redis::aio::ConnectionManager>,
}

impl RedisManager {
    pub async fn init() -> Self {
        let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis_enabled = env::var("REDIS_ENABLED")
        .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);
        if redis_enabled {
            match redis::Client::open(redis_url) {
                Ok(client) => match client.get_connection_manager().await {
                    Ok(cm) => RedisManager { client: Some(cm) },
                    Err(e) => {
                        warn!("Failed to create Redis connection manager: {}", e);
                        RedisManager { client: None }
                    }
                },
                Err(e) => {
                    warn!("Failed to open Redis client: {}", e);
                    RedisManager { client: None }
                }
            }
        } else {
            RedisManager { client: None }
        }
    }

    pub fn is_available(&self) -> bool {
        self.client.is_some()
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.get(key).await
            }
            None => Ok(None),
        }
    }

    pub async fn set_ex(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> Result<(), redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.set_ex(key, value, ttl_secs).await
            }
            None => Ok(()),
        }
    }

    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.del(key).await
            }
            None => Ok(()),
        }
    }

    pub async fn sismember(&self, key: &str, member: &str) -> Result<bool, redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.sismember(key, member).await
            }
            None => Ok(false),
        }
    }

    pub async fn sadd(&self, key: &str, members: &[String]) -> Result<(), redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.sadd(key, members).await
            }
            None => Ok(()),
        }
    }
}
