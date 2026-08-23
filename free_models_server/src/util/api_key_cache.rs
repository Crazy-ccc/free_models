use std::collections::HashSet;

use crate::db::impls::ApiKeyStoreSeaorm;

pub struct ApiKeyCache {
    keys: moka::sync::Cache<String, ()>,
    max_capacity: u64,
}

impl Clone for ApiKeyCache {
    fn clone(&self) -> Self {
        ApiKeyCache {
            keys: self.keys.clone(),
            max_capacity: self.max_capacity,
        }
    }
}

impl ApiKeyCache {
    pub async fn load_all(api_key_store: &ApiKeyStoreSeaorm, max_capacity: u64) -> Self {
        let keys = api_key_store.load_active_keys().await.unwrap_or_else(|e| {
            log::warn!("Failed to load active api keys: {}", e);
            HashSet::new()
        });

        let cache = moka::sync::Cache::builder().max_capacity(max_capacity).build();
        for k in &keys { cache.insert(k.clone(), ()); }

        let this = ApiKeyCache { keys: cache, max_capacity };
        this.warn_if_over_capacity(keys.len());

        log::info!("Loaded {} active api keys into cache", keys.len());

        this
    }

    pub async fn contains(&self, key: &str) -> bool {
        self.keys.contains_key(key)
    }

    pub async fn refresh(&self, api_key_store: &ApiKeyStoreSeaorm) {
        let keys = match api_key_store.load_active_keys().await {
            Ok(k) => k,
            Err(e) => {
                log::warn!("Failed to load active api keys: {}", e);
                return;
            }
        };

        // invalidate_all 与重新插入之间非原子，期间 contains 对有效 key 会短暂返回 false。
        // refresh 由管理员操作触发（罕见，非热路径），影响可接受；如需严格原子替换可引入 ArcSwap。
        self.keys.invalidate_all();
        for k in &keys { self.keys.insert(k.clone(), ()); }

        self.warn_if_over_capacity(keys.len());

        log::info!("Refreshed {} active api keys into cache", keys.len());
    }

    fn warn_if_over_capacity(&self, len: usize) {
        if len as u64 > self.max_capacity {
            log::warn!(
                "Active api keys ({}) exceed cache capacity ({}), LRU eviction may cause valid keys to fail",
                len, self.max_capacity
            );
        }
    }
}
