use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::db::entities::usage_log;

pub async fn create(
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
