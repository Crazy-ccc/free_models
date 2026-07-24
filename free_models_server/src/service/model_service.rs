use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::db::entities::{model_config, provider_config, provider_credential};
use crate::db::redis::RedisManager;
use crate::util::encryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderInfo {
    pub model_name: String,
    pub model_id: String,
    pub model_config_id: i32,
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
        self.protocols
            .split(',')
            .any(|p| p.trim() == protocol)
    }
}

/// 将 model_config 列表结合 provider 缓存，组装为 ModelProviderInfo 列表
/// 跳过无活跃凭证的 provider
async fn collect_model_provider_infos(
    models: Vec<model_config::Model>,
    db: &DatabaseConnection,
    provider_cache: &ProviderCache,
    encryption_key: &[u8; 32],
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    if models.is_empty() {
        return Ok(Vec::new());
    }

    let provider_ids: HashSet<i32> = models.iter().map(|m| m.provider_id).collect();
    let credentials = provider_credential::Entity::find()
        .filter(provider_credential::Column::ProviderId.is_in(provider_ids))
        .filter(provider_credential::Column::IsActive.eq(true))
        .order_by_asc(provider_credential::Column::Priority)
        .all(db)
        .await?;
    let mut cred_map: HashMap<i32, provider_credential::Model> = HashMap::new();
    for cred in credentials {
        cred_map.entry(cred.provider_id).or_insert(cred);
    }

    let mut result = Vec::new();
    for model in models {
        let provider = match get_provider_with_cache(db, model.provider_id, provider_cache).await? {
            Some(p) => p,
            None => continue,
        };
        let credential = match cred_map.get(&model.provider_id) {
            Some(c) => c,
            None => continue,
        };
        let decrypted_key = encryption::decrypt(&credential.api_key, encryption_key)?;
        result.push(ModelProviderInfo {
            model_name: model.name.clone(),
            model_id: model.model_id.clone(),
            model_config_id: model.id,
            provider_config_id: model.provider_id,
            provider_name: provider.name,
            base_url: provider.base_url,
            api_key: decrypted_key,
            provider_credential_id: credential.id,
            priority: model.priority,
            timeout: model.timeout as u64,
            protocols: model.protocols.clone(),
            context_length: model.context_length,
        });
    }
    Ok(result)
}

pub struct ModelCache {
    redis: RedisManager,
    fallback: Mutex<HashMap<String, (Instant, Vec<ModelProviderInfo>)>>,
    ttl: Duration,
    prefix: String,
}

impl ModelCache {
    pub fn new(redis: RedisManager, ttl: Duration) -> Self {
        ModelCache {
            redis,
            fallback: Mutex::new(HashMap::new()),
            ttl,
            prefix: "model_cache".to_string(),
        }
    }

    pub async fn get(&self, model_name: &str) -> Option<Vec<ModelProviderInfo>> {
        let key = format!("{}:{}", self.prefix, model_name);
        if self.redis.is_available() {
            if let Ok(Some(value)) = self.redis.get(&key).await {
                if let Ok(models) = serde_json::from_str::<Vec<ModelProviderInfo>>(&value) {
                    return Some(models);
                }
            }
        }
        let cache = self.fallback.lock().unwrap();
        if let Some((timestamp, models)) = cache.get(model_name) {
            if timestamp.elapsed() < self.ttl {
                return Some(models.clone());
            }
        }
        None
    }

    pub async fn set(&self, model_name: String, models: Vec<ModelProviderInfo>) {
        if let Ok(value) = serde_json::to_string(&models) {
            if self.redis.is_available() {
                let key = format!("{}:{}", self.prefix, model_name);
                let _ = self.redis.set_ex(&key, &value, self.ttl.as_secs()).await;
            }
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.insert(model_name, (Instant::now(), models));
    }

    pub async fn clear(&self) {
        if self.redis.is_available() {
            let _ = self.redis.del_pattern("model_cache:*").await;
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.clear();
    }
}

/// 获取所有启用的模型名称列表（去重），用于 /v1/models 接口
pub async fn get_all_model_names(
    db: &DatabaseConnection,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let models = model_config::Entity::find()
        .filter(model_config::Column::Status.eq("available"))
        .all(db)
        .await?;

    let active_credentials = provider_credential::Entity::find()
        .filter(provider_credential::Column::IsActive.eq(true))
        .all(db)
        .await?;
    let active_provider_ids: HashSet<i32> = active_credentials
        .into_iter()
        .map(|c| c.provider_id)
        .collect();

    let mut names: Vec<String> = models
        .into_iter()
        .filter(|m| active_provider_ids.contains(&m.provider_id))
        .map(|m| m.name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub base_url: String,
}

impl From<provider_config::Model> for ProviderInfo {
    fn from(m: provider_config::Model) -> Self {
        ProviderInfo {
            name: m.name,
            base_url: m.base_url,
        }
    }
}

pub struct ProviderCache {
    redis: RedisManager,
    fallback: Mutex<HashMap<i32, (Instant, ProviderInfo)>>,
    ttl: Duration,
    prefix: String,
}

impl ProviderCache {
    pub fn new(redis: RedisManager, ttl: Duration) -> Self {
        ProviderCache {
            redis,
            fallback: Mutex::new(HashMap::new()),
            ttl,
            prefix: "provider_cache".to_string(),
        }
    }

    pub async fn get(&self, provider_id: i32) -> Option<ProviderInfo> {
        let key = format!("{}:{}", self.prefix, provider_id);
        if self.redis.is_available() {
            if let Ok(Some(value)) = self.redis.get(&key).await {
                if let Ok(info) = serde_json::from_str::<ProviderInfo>(&value) {
                    return Some(info);
                }
            }
        }
        let cache = self.fallback.lock().unwrap();
        if let Some((timestamp, info)) = cache.get(&provider_id) {
            if timestamp.elapsed() < self.ttl {
                return Some(info.clone());
            }
        }
        None
    }

    pub async fn set(&self, provider_id: i32, info: ProviderInfo) {
        if let Ok(value) = serde_json::to_string(&info) {
            if self.redis.is_available() {
                let key = format!("{}:{}", self.prefix, provider_id);
                let _ = self.redis.set_ex(&key, &value, self.ttl.as_secs()).await;
            }
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.insert(provider_id, (Instant::now(), info));
    }

    pub async fn clear(&self) {
        if self.redis.is_available() {
            let _ = self.redis.del_pattern("provider_cache:*").await;
        }
        let mut cache = self.fallback.lock().unwrap();
        cache.clear();
    }
}

/// 按 id 查询 provider，命中且未过期返回缓存，否则查库并回填
pub async fn get_provider_with_cache(
    db: &DatabaseConnection,
    provider_id: i32,
    cache: &ProviderCache,
) -> Result<Option<ProviderInfo>, Box<dyn std::error::Error>> {
    if let Some(info) = cache.get(provider_id).await {
        return Ok(Some(info));
    }

    let provider = provider_config::Entity::find_by_id(provider_id)
        .one(db)
        .await?;

    let result = provider.map(ProviderInfo::from);

    if let Some(ref info) = result {
        cache.set(provider_id, info.clone()).await;
    }

    Ok(result)
}

/// 返回所有 status='available' 且关联 provider 有活跃凭证的模型，按 priority 升序
/// 使用 ModelCache 的特殊 key "__all__" 缓存，避免每次请求都查库
pub async fn get_all_available_models_by_priority(
    db: &DatabaseConnection,
    model_cache: &ModelCache,
    provider_cache: &ProviderCache,
    encryption_key: &[u8; 32],
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    const ALL_KEY: &str = "__all__";

    if let Some(cached) = model_cache.get(ALL_KEY).await {
        return Ok(cached);
    }

    let models = model_config::Entity::find()
        .filter(model_config::Column::Status.eq("available"))
        .order_by_asc(model_config::Column::Priority)
        .all(db)
        .await?;

    let result = collect_model_provider_infos(models, db, provider_cache, encryption_key).await?;
    model_cache.set(ALL_KEY.to_string(), result.clone()).await;
    Ok(result)
}