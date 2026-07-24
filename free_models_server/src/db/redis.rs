use log::warn;
use redis::AsyncCommands;

#[derive(Clone)]
pub struct RedisManager {
    client: Option<redis::aio::ConnectionManager>,
}

impl RedisManager {
    pub async fn new(redis_url: &str) -> Self {
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
    }

    pub fn disabled() -> Self {
        RedisManager { client: None }
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

    pub async fn exists(&self, key: &str) -> Result<bool, redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.exists(key).await
            }
            None => Ok(false),
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

    pub async fn _srem(&self, key: &str, members: &[String]) -> Result<(), redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                conn.srem(key, members).await
            }
            None => Ok(()),
        }
    }

    pub async fn del_pattern(&self, pattern: &str) -> Result<(), redis::RedisError> {
        match &self.client {
            Some(client) => {
                let mut conn = client.clone();
                let keys: Vec<String> = conn.keys(pattern).await?;
                if !keys.is_empty() {
                    let _: i64 = conn.del(keys).await?;
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}
