use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::db::entities::{model_config, provider_config};

#[derive(Debug, Clone, Serialize)]
pub struct ModelProviderInfo {
    pub model_name: String,
    pub model_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub api_key: String,
    pub priority: i32,
    pub timeout: u64,
}

/// 将 model_config 列表结合 provider 缓存，组装为 ModelProviderInfo 列表
/// 跳过 provider 未激活的记录
async fn collect_model_provider_infos(
    models: Vec<model_config::Model>,
    db: &DatabaseConnection,
    provider_cache: &ProviderCache,
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();
    for model in models {
        if let Some(provider) = get_provider_with_cache(db, model.provider_id, provider_cache).await? {
            if !provider.is_active {
                continue;
            }
            result.push(ModelProviderInfo {
                model_name: model.name.clone(),
                model_id: model.model_id.clone(),
                provider_name: provider.name,
                base_url: provider.base_url,
                api_key: provider.api_key,
                priority: model.priority,
                timeout: model.timeout as u64,
            });
        }
    }
    Ok(result)
}

/// 按模型名称查询所有启用的 model_config，通过 ProviderCache 获取 provider，按 priority 升序排列
#[allow(dead_code)]
pub async fn get_models_by_name(
    db: &DatabaseConnection,
    model_name: &str,
    provider_cache: &ProviderCache,
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    let models = model_config::Entity::find()
        .filter(model_config::Column::Name.eq(model_name))
        .filter(model_config::Column::IsActive.eq(true))
        .order_by_asc(model_config::Column::Priority)
        .all(db)
        .await?;

    collect_model_provider_infos(models, db, provider_cache).await
}

pub struct ModelCache {
    cache: Mutex<HashMap<String, (Instant, Vec<ModelProviderInfo>)>>,
    ttl: Duration,
}

impl ModelCache {
    pub fn new() -> Self {
        ModelCache {
            cache: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(30),
        }
    }

    pub fn get(&self, model_name: &str) -> Option<Vec<ModelProviderInfo>> {
        let cache = self.cache.lock().unwrap();
        if let Some((timestamp, models)) = cache.get(model_name) {
            if timestamp.elapsed() < self.ttl {
                return Some(models.clone());
            }
        }
        None
    }

    pub fn set(&self, model_name: String, models: Vec<ModelProviderInfo>) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(model_name, (Instant::now(), models));
    }
}

/// 带缓存的模型查询
#[allow(dead_code)]
pub async fn get_models_with_cache(
    db: &DatabaseConnection,
    model_name: &str,
    model_cache: &ModelCache,
    provider_cache: &ProviderCache,
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    if let Some(cached) = model_cache.get(model_name) {
        return Ok(cached);
    }

    let models = get_models_by_name(db, model_name, provider_cache).await?;
    model_cache.set(model_name.to_string(), models.clone());
    Ok(models)
}

/// 获取所有启用的模型名称列表（去重），用于 /v1/models 接口
pub async fn get_all_model_names(
    db: &DatabaseConnection,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let models = model_config::Entity::find()
        .filter(model_config::Column::IsActive.eq(true))
        .join(
            JoinType::InnerJoin,
            model_config::Relation::ProviderConfig.def(),
        )
        .filter(provider_config::Column::IsActive.eq(true))
        .all(db)
        .await?;

    let mut names: Vec<String> = models.into_iter().map(|m| m.name).collect();
    names.sort();
    names.dedup();
    Ok(names)
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub is_active: bool,
}

impl From<provider_config::Model> for ProviderInfo {
    fn from(m: provider_config::Model) -> Self {
        ProviderInfo {
            name: m.name,
            base_url: m.base_url,
            api_key: m.api_key,
            is_active: m.is_active,
        }
    }
}

pub struct ProviderCache {
    cache: Mutex<HashMap<i32, (Instant, ProviderInfo)>>,
    ttl: Duration,
}

impl ProviderCache {
    pub fn new() -> Self {
        ProviderCache {
            cache: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(600),
        }
    }
}

/// 按 id 查询 provider，命中且未过期返回缓存，否则查库并回填
pub async fn get_provider_with_cache(
    db: &DatabaseConnection,
    provider_id: i32,
    cache: &ProviderCache,
) -> Result<Option<ProviderInfo>, Box<dyn std::error::Error>> {
    {
        let cache_guard = cache.cache.lock().unwrap();
        if let Some((timestamp, info)) = cache_guard.get(&provider_id) {
            if timestamp.elapsed() < cache.ttl {
                return Ok(Some(info.clone()));
            }
        }
    }

    let provider = provider_config::Entity::find_by_id(provider_id)
        .one(db)
        .await?;

    let result = provider.map(ProviderInfo::from);

    if let Some(ref info) = result {
        let mut cache_guard = cache.cache.lock().unwrap();
        cache_guard.insert(provider_id, (Instant::now(), info.clone()));
    }

    Ok(result)
}

/// 返回所有 model_config.is_active=true 且关联 provider_config.is_active=true 的模型，按 priority 升序
/// 使用 ModelCache 的特殊 key "__all__" 缓存，避免每次请求都 JOIN 查库
pub async fn get_all_available_models_by_priority(
    db: &DatabaseConnection,
    model_cache: &ModelCache,
    provider_cache: &ProviderCache,
) -> Result<Vec<ModelProviderInfo>, Box<dyn std::error::Error>> {
    const ALL_KEY: &str = "__all__";

    if let Some(cached) = model_cache.get(ALL_KEY) {
        return Ok(cached);
    }

    let models = model_config::Entity::find()
        .filter(model_config::Column::IsActive.eq(true))
        .order_by_asc(model_config::Column::Priority)
        .all(db)
        .await?;

    let result = collect_model_provider_infos(models, db, provider_cache).await?;
    model_cache.set(ALL_KEY.to_string(), result.clone());
    Ok(result)
}