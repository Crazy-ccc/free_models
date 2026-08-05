use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::cache::RedisManager;
use crate::db::Database;
use crate::db::entities::model_config::Model as ModelConfig;
use crate::db::entities::provider_credential::Model as ProviderCredential;
use crate::db::entities::provider_model_map::Model as ProviderModelMap;
use crate::db::entities::provider_config::Model as ProviderConfig;
use crate::db::StoreError;
use serde::{Deserialize, Serialize};
use crate::util::encryption;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelScheduleInfo {
    pub model_name: String,
    pub model_config_id: i32,
    pub priority: i32,
    pub maps: Vec<ModelProviderMap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderMap {
    pub provider_model_map_id: i32,
    pub provider_config_id: i32,
    pub provider_name: String,
    pub base_url: String,
    pub model_id: String,
    pub priority: i32,
    pub timeout: u64,
    pub protocols: String,
    pub context_length: i32,
    pub credentials: Vec<CredentialInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialInfo {
    pub provider_credential_id: i32,
    pub api_key: String,
    pub priority: i32,
    pub quota_exhausted: bool,
}

impl ModelScheduleInfo {
    pub fn supports_protocol(&self, protocol: &str) -> bool {
        self.maps.iter().any(|m| m.supports_protocol(protocol))
    }
}

impl ModelProviderMap {
    pub fn supports_protocol(&self, protocol: &str) -> bool {
        self.protocols.split(',').any(|p| p.trim() == protocol)
    }
}

/// SchedulerCache: Redis + local secondary cache for ModelScheduleInfo
pub struct SchedulerCache {
    cache_store: Arc<RedisManager>,
    fallback: moka::future::Cache<String, Vec<ModelScheduleInfo>>,
    ttl: Duration,
}

impl SchedulerCache {
    pub fn new(cache_store: Arc<RedisManager>, ttl: Duration) -> Self {
        SchedulerCache {
            cache_store,
            fallback: moka::future::Cache::builder().time_to_live(ttl).build(),
            ttl,
        }
    }

    pub async fn get_all(&self) -> Option<Vec<ModelScheduleInfo>> {
        self.get("all").await
    }

    pub async fn set_all(&self, models: Vec<ModelScheduleInfo>) {
        self.set("all".to_string(), models).await;
    }

    pub async fn clear(&self) {
        let _ = self.cache_store.del("app:free_models:scheduler:all").await;
        self.fallback.invalidate_all();
    }

    async fn get(&self, key: &str) -> Option<Vec<ModelScheduleInfo>> {
        let full_key = format!("app:free_models:scheduler:{}", key);
        if self.cache_store.is_available() {
            if let Ok(Some(value)) = self.cache_store.get(&full_key).await {
                if let Ok(models) = serde_json::from_str::<Vec<ModelScheduleInfo>>(&value) {
                    return Some(models);
                }
            }
        }
        self.fallback.get(key).await
    }

    async fn set(&self, key: String, models: Vec<ModelScheduleInfo>) {
        if let Ok(value) = serde_json::to_string(&models) {
            if self.cache_store.is_available() {
                let full_key = format!("app:free_models:scheduler:{}", key);
                let _ = self.cache_store.set_ex(&full_key, &value, self.ttl.as_secs()).await;
            }
        }
        self.fallback.insert(key, models).await;
    }
}

/// 返回所有活跃模型及其嵌套的映射与凭证信息
///
/// 每个模型生成一条 ModelScheduleInfo，模型间按 model_config priority ASC 排序
/// （list_all_active_ordered 已 order_by_asc）。模型内的 maps 按 map.priority ASC
/// 排序，每个 map 内的 credentials 按 priority ASC 排序（sort_by_key(|c| c.priority)）。
/// 1. 先查缓存，缓存命中直接返回
/// 2. 批量查询所有活跃 model_config、provider_model_map、provider、credential
/// 3. 按模型分组，调用 build_model_info 构造 ModelScheduleInfo
pub async fn schedule_all_available(
    database: &Database,
    encryption_key: &[u8; 32],
    cache: &SchedulerCache,
) -> Result<Vec<ModelScheduleInfo>, StoreError> {
    // Try cache first
    if let Some(cached) = cache.get_all().await {
        return Ok(cached);
    }

    // 批量查询所有活跃模型
    let models = database.model_configs.list_all_active_ordered().await?;
    if models.is_empty() {
        cache.set_all(Vec::new()).await;
        return Ok(Vec::new());
    }

    // 批量查询所有活跃映射
    let all_maps = database.provider_model_maps.list_filtered(None, None).await?;

    // 收集所有涉及的 provider_id
    let provider_ids: HashSet<i32> = all_maps.iter().map(|m| m.provider_id).collect();

    // 批量查询 providers 和 credentials
    let providers_map = database.provider_configs.list_by_ids(&provider_ids).await?;
    let all_credentials = database.provider_credentials
        .list_active_by_provider_ids(&provider_ids)
        .await?;

    // 按 model_id 分组映射
    let mut maps_by_model: HashMap<i32, Vec<&ProviderModelMap>> = HashMap::new();
    for map_entry in &all_maps {
        maps_by_model.entry(map_entry.model_id).or_default().push(map_entry);
    }

    let mut result = Vec::new();
    for model in &models {
        let maps = maps_by_model.get(&model.id).cloned().unwrap_or_default();
        if let Some(info) = build_model_info(model, &maps, &providers_map, &all_credentials, encryption_key)? {
            result.push(info);
        }
    }

    cache.set_all(result.clone()).await;
    Ok(result)
}

/// 阶段一：映射 → provider 查找
/// 返回 (map_entry, provider_config) 对，跳过无对应 provider 的映射
fn resolve_providers<'a>(
    maps: &'a [&'a ProviderModelMap],
    providers: &'a HashMap<i32, ProviderConfig>,
) -> Vec<(&'a ProviderModelMap, &'a ProviderConfig)> {
    maps.iter()
        .filter_map(|&map_entry| {
            providers.get(&map_entry.provider_id).map(|p| (map_entry, p))
        })
        .collect()
}

/// 阶段二：凭证查找
/// 返回按 provider_id 分组的活跃凭证列表
fn resolve_credentials(
    provider_ids: &HashSet<i32>,
    all_credentials: &[ProviderCredential],
) -> HashMap<i32, Vec<ProviderCredential>> {
    let mut cred_map: HashMap<i32, Vec<ProviderCredential>> = HashMap::new();
    for cred in all_credentials {
        if provider_ids.contains(&cred.provider_id) {
            cred_map.entry(cred.provider_id).or_default().push(cred.clone());
        }
    }
    cred_map
}

/// 阶段三：为单个模型构造 ModelScheduleInfo
/// 无任何可用映射时返回 None（该模型不可用，不进入结果列表）
fn build_model_info(
    model_config_item: &ModelConfig,
    maps: &[&ProviderModelMap],
    providers: &HashMap<i32, ProviderConfig>,
    all_credentials: &[ProviderCredential],
    encryption_key: &[u8; 32],
) -> Result<Option<ModelScheduleInfo>, StoreError> {
    let resolved = resolve_providers(maps, providers);
    if resolved.is_empty() {
        return Ok(None);
    }

    let provider_ids: HashSet<i32> = resolved.iter().map(|(_, p)| p.id).collect();
    let cred_map = resolve_credentials(&provider_ids, all_credentials);

    let mut model_maps: Vec<ModelProviderMap> = Vec::new();
    for (map_entry, provider) in &resolved {
        let cred_list = cred_map.get(&map_entry.provider_id).cloned().unwrap_or_default();
        let mut sorted_creds = cred_list;
        sorted_creds.sort_by_key(|c| c.priority);

        let mut credentials = Vec::with_capacity(sorted_creds.len());
        for credential in &sorted_creds {
            let decrypted_key = encryption::decrypt(&credential.api_key, encryption_key)
                .map_err(StoreError::Database)?;
            credentials.push(CredentialInfo {
                provider_credential_id: credential.id,
                api_key: decrypted_key,
                priority: credential.priority,
                quota_exhausted: credential.quota_exhausted,
            });
        }

        model_maps.push(ModelProviderMap {
            provider_model_map_id: map_entry.id,
            provider_config_id: provider.id,
            provider_name: provider.name.clone(),
            base_url: provider.base_url.clone(),
            model_id: map_entry.provider_model_id.clone(),
            priority: map_entry.priority,
            timeout: map_entry.timeout.unwrap_or(model_config_item.timeout) as u64,
            protocols: map_entry.protocols.clone(),
            context_length: map_entry
                .context_length
                .unwrap_or(model_config_item.context_length),
            credentials,
        });
    }

    // maps 按 priority ASC 排序（priority 小者在前）
    model_maps.sort_by_key(|m| m.priority);

    Ok(Some(ModelScheduleInfo {
        model_name: model_config_item.name.clone(),
        model_config_id: model_config_item.id,
        priority: model_config_item.priority,
        maps: model_maps,
    }))
}

/// 获取所有可用模型名称列表（去重）
pub async fn get_all_model_names(
    model_store: &crate::db::impls::ModelConfigStoreSeaorm,
    pmm_store: &crate::db::impls::ProviderModelMapStoreSeaorm,
    cred_store: &crate::db::impls::ProviderCredentialStoreSeaorm,
) -> Result<Vec<String>, StoreError> {
    let models = model_store.list_all_active_ordered().await?;
    let mappings = pmm_store.list_filtered(None, None).await?;
    let active_creds = cred_store.list().await?;

    let active_provider_ids: HashSet<i32> = active_creds
        .into_iter()
        .filter(|c| c.is_active)
        .map(|c| c.provider_id)
        .collect();

    let active_model_ids: HashSet<i32> = mappings
        .into_iter()
        .filter(|m| active_provider_ids.contains(&m.provider_id))
        .map(|m| m.model_id)
        .collect();

    let mut names: Vec<String> = models
        .into_iter()
        .filter(|m| active_model_ids.contains(&m.id))
        .map(|m| m.name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

