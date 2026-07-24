use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::db::redis::RedisManager;

#[derive(Clone)]
pub struct PriorityPenalty {
    redis: RedisManager,
    ttl: Duration,
    local_cache: Arc<Mutex<HashMap<String, (Instant, bool)>>>,
    fallback: Arc<Mutex<HashMap<String, Instant>>>,
}

impl PriorityPenalty {
    pub fn new(redis: RedisManager, ttl: Duration) -> Self {
        PriorityPenalty {
            redis,
            ttl,
            local_cache: Arc::new(Mutex::new(HashMap::new())),
            fallback: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn ttl_secs(&self) -> u64 {
        self.ttl.as_secs()
    }

    fn key(model_name: &str, provider_name: &str) -> String {
        format!("{}|{}", model_name, provider_name)
    }

    pub async fn penalize(&self, model_name: &str, provider_name: &str) {
        let key = Self::key(model_name, provider_name);
        let redis_key = format!("penalty:{}|{}", model_name, provider_name);

        if self.redis.is_available() {
            let _ = self.redis.set_ex(&redis_key, "1", self.ttl.as_secs()).await;
        }

        {
            let mut local_cache = self.local_cache.lock().unwrap();
            local_cache.insert(key.clone(), (Instant::now(), true));
        }

        {
            let mut fallback = self.fallback.lock().unwrap();
            fallback.insert(key, Instant::now() + self.ttl);
        }
    }

    pub async fn is_penalized(&self, model_name: &str, provider_name: &str) -> bool {
        let key = Self::key(model_name, provider_name);
        let redis_key = format!("penalty:{}|{}", model_name, provider_name);
        let now = Instant::now();
        let local_ttl = Duration::from_secs(30);

        // 先查本地二级缓存
        {
            let local_cache = self.local_cache.lock().unwrap();
            if let Some((timestamp, value)) = local_cache.get(&key) {
                if now.duration_since(*timestamp) < local_ttl {
                    return *value;
                }
            }
        }

        // 本地缓存未命中或已过期
        if self.redis.is_available() {
            match self.redis.exists(&redis_key).await {
                Ok(true) => {
                    let mut local_cache = self.local_cache.lock().unwrap();
                    local_cache.insert(key, (Instant::now(), true));
                    true
                }
                Ok(false) => {
                    let mut local_cache = self.local_cache.lock().unwrap();
                    local_cache.insert(key, (Instant::now(), false));
                    false
                }
                Err(_) => self.fallback_is_penalized(&key, now),
            }
        } else {
            self.fallback_is_penalized(&key, now)
        }
    }

    fn fallback_is_penalized(&self, key: &str, now: Instant) -> bool {
        let mut fallback = self.fallback.lock().unwrap();
        match fallback.get(key) {
            Some(expiry) => {
                if *expiry > now {
                    true
                } else {
                    fallback.remove(key);
                    false
                }
            }
            None => false,
        }
    }

    pub async fn sort_penalized_last<T, F>(&self, items: Vec<T>, key_of: F) -> Vec<T>
    where
        F: Fn(&T) -> (String, String),
    {
        let mut penalized: Vec<T> = Vec::new();
        let mut rest: Vec<T> = Vec::new();
        for item in items {
            let (model_name, provider_name) = key_of(&item);
            if self.is_penalized(&model_name, &provider_name).await {
                penalized.push(item);
            } else {
                rest.push(item);
            }
        }
        rest.extend(penalized);
        rest
    }
}
