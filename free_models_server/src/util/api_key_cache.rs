use std::collections::HashSet;
use std::sync::Arc;

use crate::db::impls::ApiKeyStoreSeaorm;
use crate::cache::RedisManager;

pub struct ApiKeyCache {
    cache_store: Arc<RedisManager>,
    fallback: moka::sync::Cache<String, ()>,
    redis_key: String,
}

impl Clone for ApiKeyCache {
    fn clone(&self) -> Self {
        ApiKeyCache {
            cache_store: self.cache_store.clone(),
            fallback: self.fallback.clone(),
            redis_key: self.redis_key.clone(),
        }
    }
}

impl ApiKeyCache {
    pub async fn load_all(api_key_store: &ApiKeyStoreSeaorm, cache_store: Arc<RedisManager>, max_capacity: u64) -> Self {
        let keys = api_key_store.load_active_keys().await.unwrap_or_else(|e| {
            log::warn!("Failed to load active api keys: {}", e);
            HashSet::new()
        });

        let fallback = moka::sync::Cache::builder().max_capacity(max_capacity).build();
        for k in &keys { fallback.insert(k.clone(), ()); }

        let this = ApiKeyCache {
            cache_store,
            fallback,
            redis_key: "app:free_models:api_keys:active".to_string(),
        };
        this.sync_to_redis(&keys).await;

        log::info!("Loaded {} active api keys into cache", keys.len());

        this
    }

    pub async fn contains(&self, key: &str) -> bool {
        match self.cache_store.sismember(&self.redis_key, key).await {
            Ok(true) => true,
            Ok(false) => {
                if self.cache_store.is_available() {
                    false
                } else {
                    self.fallback.contains_key(key)
                }
            }
            Err(_) => self.fallback.contains_key(key),
        }
    }

    pub async fn refresh(&self, api_key_store: &ApiKeyStoreSeaorm) {
        let keys = match api_key_store.load_active_keys().await {
            Ok(k) => k,
            Err(e) => {
                log::warn!("Failed to load active api keys: {}", e);
                return;
            }
        };

        // 注意：invalidate_all 与重新插入之间非原子，期间 contains 对有效 key 会返回 false。
        // 该 fallback 仅在 Redis 不可用时被查询，且 refresh 由管理员操作触发（罕见，非热路径），影响可接受。
        // 如需严格原子替换，可构造新 Cache 并通过 ArcSwap 原子交换。
        self.fallback.invalidate_all();
        for k in &keys { self.fallback.insert(k.clone(), ()); }
        let fallback_len = keys.len();

        self.sync_to_redis(&keys).await;

        log::info!("Refreshed {} active api keys into cache", fallback_len);
    }

    async fn sync_to_redis(&self, keys: &HashSet<String>) {
        if self.cache_store.is_available() {
            if let Err(e) = self.cache_store.del(&self.redis_key).await {
                log::warn!("Failed to del cache key {}: {}", self.redis_key, e);
            }
            let keys_vec: Vec<String> = keys.iter().cloned().collect();
            if let Err(e) = self.cache_store.sadd(&self.redis_key, &keys_vec).await {
                log::warn!("Failed to sadd cache key {}: {}", self.redis_key, e);
            }
        }
    }
}
