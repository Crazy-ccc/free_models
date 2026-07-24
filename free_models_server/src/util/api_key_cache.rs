use std::collections::HashSet;
use std::sync::Mutex;

use sea_orm::{DatabaseConnection};

use crate::db::redis::RedisManager;
use crate::service::api_key_service;

pub struct ApiKeyCache {
    redis: RedisManager,
    fallback: Mutex<HashSet<String>>,
    redis_key: String,
}

impl Clone for ApiKeyCache {
    fn clone(&self) -> Self {
        let fallback = self.fallback.lock().unwrap().clone();
        ApiKeyCache {
            redis: self.redis.clone(),
            fallback: Mutex::new(fallback),
            redis_key: self.redis_key.clone(),
        }
    }
}

impl ApiKeyCache {
    /// 从数据库一次性加载所有 is_active = true 的 key_value
    pub async fn load_all(db: &DatabaseConnection, redis: RedisManager) -> Self {
        let keys = api_key_service::load_active_keys(db).await;

        let keys_vec: Vec<String> = keys.iter().cloned().collect();

        if redis.is_available() {
            if let Err(e) = redis.del("api_keys:active").await {
                log::warn!("Failed to del Redis key api_keys:active: {}", e);
            }
            if let Err(e) = redis.sadd("api_keys:active", &keys_vec).await {
                log::warn!("Failed to sadd Redis key api_keys:active: {}", e);
            }
        }

        log::info!("Loaded {} active api keys into cache", keys.len());

        ApiKeyCache {
            redis,
            fallback: Mutex::new(keys),
            redis_key: "api_keys:active".to_string(),
        }
    }

    /// 检查 key 是否存在于缓存中
    pub async fn contains(&self, key: &str) -> bool {
        match self.redis.sismember(&self.redis_key, key).await {
            Ok(true) => true,
            Ok(false) => {
                if self.redis.is_available() {
                    false
                } else {
                    self.fallback.lock().unwrap().contains(key)
                }
            }
            Err(_) => self.fallback.lock().unwrap().contains(key),
        }
    }

    /// 重新从数据库加载并刷新缓存
    pub async fn refresh(&self, db: &DatabaseConnection) {
        let keys = api_key_service::load_active_keys(db).await;

        let mut fallback = self.fallback.lock().unwrap();
        *fallback = keys.clone();
        let fallback_len = fallback.len();
        drop(fallback);

        if self.redis.is_available() {
            if let Err(e) = self.redis.del(&self.redis_key).await {
                log::warn!("Failed to del Redis key {}: {}", self.redis_key, e);
            }
            let keys_vec: Vec<String> = keys.iter().cloned().collect();
            if let Err(e) = self.redis.sadd(&self.redis_key, &keys_vec).await {
                log::warn!("Failed to sadd Redis key {}: {}", self.redis_key, e);
            }
        }

        log::info!("Refreshed {} active api keys into cache", fallback_len);
    }
}
