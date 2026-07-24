use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

use crate::db::entities::usage_log;

/// 批量插入用的数据记录，所有权完整，可跨线程发送
#[derive(Clone)]
pub struct UsageLogInsert {
    pub api_key_id: Option<i32>,
    pub api_key_name: Option<String>,
    pub model_config_id: i32,
    pub provider_config_id: i32,
    pub provider_credential_id: i32,
    pub model_name: String,
    pub provider_name: String,
    pub protocol: String,
    pub status: String,
    pub error_message: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub cache_miss_tokens: i32,
    pub duration_ms: i32,
    pub is_stream: bool,
}

/// 批量插入多条 usage_log 记录
pub async fn create_batch(
    db: &DatabaseConnection,
    records: &[UsageLogInsert],
) -> Result<(), sea_orm::DbErr> {
    if records.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().naive_utc();
    let models: Vec<usage_log::ActiveModel> = records
        .iter()
        .map(|r| usage_log::ActiveModel {
            api_key_id: Set(r.api_key_id),
            api_key_name: Set(r.api_key_name.clone()),
            model_config_id: Set(Some(r.model_config_id)),
            provider_config_id: Set(Some(r.provider_config_id)),
            provider_credential_id: Set(Some(r.provider_credential_id)),
            model_name: Set(r.model_name.clone()),
            provider_name: Set(r.provider_name.clone()),
            protocol: Set(r.protocol.clone()),
            status: Set(r.status.clone()),
            error_message: Set(r.error_message.clone()),
            prompt_tokens: Set(r.prompt_tokens),
            completion_tokens: Set(r.completion_tokens),
            total_tokens: Set(r.total_tokens),
            cache_hit_tokens: Set(r.cache_hit_tokens),
            cache_miss_tokens: Set(r.cache_miss_tokens),
            duration_ms: Set(r.duration_ms),
            is_stream: Set(r.is_stream),
            request_timestamp: Set(now),
            ..Default::default()
        })
        .collect();
    usage_log::Entity::insert_many(models).exec(db).await?;
    Ok(())
}

pub async fn _create(
    db: &DatabaseConnection,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    model_config_id: Option<i32>,
    provider_config_id: Option<i32>,
    provider_credential_id: Option<i32>,
    model_name: &str,
    provider_name: &str,
    protocol: &str,
    status: &str,
    error_message: Option<&str>,
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    cache_hit_tokens: i32,
    cache_miss_tokens: i32,
    duration_ms: i32,
    is_stream: bool,
) -> Result<usage_log::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().naive_utc();
    let log = usage_log::ActiveModel {
        api_key_id: Set(api_key_id),
        api_key_name: Set(api_key_name.map(|s| s.to_string())),
        model_config_id: Set(model_config_id),
        provider_config_id: Set(provider_config_id),
        provider_credential_id: Set(provider_credential_id),
        model_name: Set(model_name.to_string()),
        provider_name: Set(provider_name.to_string()),
        protocol: Set(protocol.to_string()),
        status: Set(status.to_string()),
        error_message: Set(error_message.map(|s| s.to_string())),
        prompt_tokens: Set(prompt_tokens),
        completion_tokens: Set(completion_tokens),
        total_tokens: Set(total_tokens),
        cache_hit_tokens: Set(cache_hit_tokens),
        cache_miss_tokens: Set(cache_miss_tokens),
        duration_ms: Set(duration_ms),
        is_stream: Set(is_stream),
        request_timestamp: Set(now),
        ..Default::default()
    };
    log.insert(db).await
}
