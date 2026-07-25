use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::db::entities::{model_config, provider_credential, provider_model_map};
use crate::db::redis::RedisManager;
use crate::service::model_service_ext;
use crate::service::provider_credential_service;
use crate::service::provider_model_map_service;
use crate::service::provider_service;
use crate::util::encryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderInfo {
    pub model_name: String,
    pub model_id: String,
    pub model_config_id: i32,
    pub provider_model_map_id: i32,
    pub provider_config_id: i32,
    pub provider_name: String,
    pub base_url: String,
    pub api_key: String,
    pub provider_credential_id: i32,
    pub priority: i32,
    pub timeout: u64,
    pub protocols: String,
    pub context_length: i32,
}

impl ModelProviderInfo {
    pub fn supports_protocol(&self, protocol: &str) -> bool {
        self.protocols.split(',').any(|p| p.trim() == protocol)
    }
}

/// SchedulerCache: Redis + local secondary cache for ModelProviderInfo
pub struct SchedulerCache {
    redis: RedisManager,
    fallback: Mutex<HashMap<String, (Instant, Vec<ModelProviderInfo>)>>,
    ttl: Duration,
    prefix: String,
}

impl SchedulerCache {
    pub fn new(redis: RedisManager, ttl: Duration) -> Self {
        SchedulerCache {
            redis,
            fallback: Mutex::new(HashMap::new()),
            ttl,
            prefix: "scheduler_cache".to_string(),
        }
    }

    pub async fn get_all(&self) -> Option<Vec<ModelProviderInfo>> {
        self.get("all").await
    }

    pub async fn set_all(&self, models: Vec<ModelProviderInfo>) {
        self.set("all".to_string(), models).await;
    }

    pub async fn clear(&self) {
        if self.redis.is_available() {
            let _ = self.redis.del_pattern("scheduler_cache:*").await;
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.clear();
    }

    async fn get(&self, key: &str) -> Option<Vec<ModelProviderInfo>> {
        let full_key = format!("{}:{}", self.prefix, key);
        if self.redis.is_available() {
            if let Ok(Some(value)) = self.redis.get(&full_key).await {
                if let Ok(models) = serde_json::from_str::<Vec<ModelProviderInfo>>(&value) {
                    return Some(models);
                }
            }
        }
        let cache = self.fallback.lock().unwrap();
        if let Some((timestamp, models)) = cache.get(key) {
            if timestamp.elapsed() < self.ttl {
                return Some(models.clone());
            }
        }
        None
    }

    async fn set(&self, key: String, models: Vec<ModelProviderInfo>) {
        if let Ok(value) = serde_json::to_string(&models) {
            if self.redis.is_available() {
                let full_key = format!("{}:{}", self.prefix, key);
                let _ = self.redis.set_ex(&full_key, &value, self.ttl.as_secs()).await;
            }
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.insert(key, (Instant::now(), models));
    }
}

/// 返回所有活跃模型的全部可用 provider
///
/// 按 model_config priority ASC 排序，每个模型内按 map priority ASC 排序。
/// 1. 先查缓存，缓存命中直接返回
/// 2. 通过 service 层查询所有活跃 model_config
/// 3. 对每个 model 查找活跃 provider_model_map、provider、credential，解密后构造 ModelProviderInfo
pub async fn schedule_all_available(
    db: &DatabaseConnection,
    encryption_key: &[u8; 32],
    cache: &SchedulerCache,
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    // Try cache first
    if let Some(cached) = cache.get_all().await {
        return Ok(cached);
    }

    // Query via service layer
    let models = model_service_ext::list_all_active_ordered(db).await?;

    let mut result = Vec::new();
    for model in &models {
        let maps = provider_model_map_service::list_active_by_model_id(db, model.id).await?;
        let infos = build_provider_infos(model, &maps, db, encryption_key).await?;
        result.extend(infos);
    }

    cache.set_all(result.clone()).await;
    Ok(result)
}

/// 内部辅助：根据 model_config 和 provider_model_map 列表，查找 provider + credentials，构造 ModelProviderInfo 列表
async fn build_provider_infos(
    model_config_item: &model_config::Model,
    maps: &[provider_model_map::Model],
    db: &DatabaseConnection,
    encryption_key: &[u8; 32],
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    if maps.is_empty() {
        return Ok(Vec::new());
    }

    // 收集所有涉及的 provider_id
    let provider_ids: HashSet<i32> = maps.iter().map(|m| m.provider_id).collect();

    // 通过 service 层查找所有活跃凭证，按 provider_id 分组取第一条（priority 最高）
    let credentials = provider_credential_service::list_active_by_provider_ids(db, provider_ids.clone()).await?;
    let mut cred_map: HashMap<i32, provider_credential::Model> = HashMap::new();
    for cred in credentials {
        cred_map.entry(cred.provider_id).or_insert(cred);
    }

    // 通过 service 层查找所有 provider 配置
    let providers = provider_service::list_by_ids(db, provider_ids).await?;

    let mut result = Vec::with_capacity(maps.len());
    for map_entry in maps {
        let provider = match providers.get(&map_entry.provider_id) {
            Some(p) => p,
            None => continue,
        };
        let credential = match cred_map.get(&map_entry.provider_id) {
            Some(c) => c,
            None => continue,
        };
        let decrypted_key = encryption::decrypt(&credential.api_key, encryption_key)?;

        result.push(ModelProviderInfo {
            model_name: model_config_item.name.clone(),
            model_id: map_entry.provider_model_id.clone(),
            model_config_id: model_config_item.id,
            provider_model_map_id: map_entry.id,
            provider_config_id: provider.id,
            provider_name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            api_key: decrypted_key,
            provider_credential_id: credential.id,
            priority: map_entry.priority,
            timeout: map_entry.timeout.unwrap_or(model_config_item.timeout) as u64,
            protocols: map_entry.protocols.clone(),
            context_length: map_entry
                .context_length
                .unwrap_or(model_config_item.context_length),
        });
    }
    Ok(result)
}
