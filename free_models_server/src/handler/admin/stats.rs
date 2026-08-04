use std::collections::{HashMap, HashSet};

use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::db::StoreError;
use crate::response;
use crate::AppState;

/// 统计真正可用的模型数：活跃 model_config + 活跃 provider_model_map + 活跃 provider_credential
async fn count_available_models(state: &web::Data<AppState>) -> Result<u64, StoreError> {
    let models = state.database.model_configs.list_all_active_ordered().await?;

    // 批量查询所有映射（一次 DB 查询替代按 model 逐次查询）
    let all_maps = state.database.provider_model_maps.list_filtered(None, None).await?;

    // 仅保留活跃映射，按 model_id 分组收集 provider_id
    let mut active_maps_by_model: HashMap<i32, HashSet<i32>> = HashMap::new();
    let mut all_provider_ids: HashSet<i32> = HashSet::new();
    for m in &all_maps {
        if m.is_active {
            active_maps_by_model.entry(m.model_id).or_default().insert(m.provider_id);
            all_provider_ids.insert(m.provider_id);
        }
    }

    if all_provider_ids.is_empty() {
        return Ok(0);
    }

    let credentials = state.database.provider_credentials.list_active_by_provider_ids(&all_provider_ids).await?;
    let providers_with_creds: HashSet<i32> = credentials.iter().map(|c| c.provider_id).collect();

    let count = models.iter()
        .filter(|model| {
            active_maps_by_model.get(&model.id)
                .map_or(false, |pids| pids.iter().any(|pid| providers_with_creds.contains(pid)))
        })
        .count() as u64;
    Ok(count)
}

pub async fn get_service_status(state: web::Data<AppState>) -> HttpResponse {
    let (active_models, total_models, active_credentials, total_providers, active_api_keys, total_api_keys) =
        match tokio::try_join!(
            count_available_models(&state),
            state.database.model_configs.list(),
            state.database.provider_credentials.list(),
            state.database.provider_configs.list(),
            state.database.api_keys.load_active_keys(),
            state.database.api_keys.list(),
        ) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Service status query failed: {}", e);
                return response::db_error();
            }
        };

    let unique_providers: HashSet<i32> = active_credentials.iter().map(|c| c.provider_id).collect();
    let am = active_models;
    let tm = total_models.len() as u64;
    let ap = unique_providers.len() as u64;
    let tp = total_providers.len() as u64;
    let ak = active_api_keys.len() as u64;
    let tk = total_api_keys.len() as u64;

    let penalties = state.priority_penalty.list_blocked_models();
    let penalties_json: Vec<serde_json::Value> = penalties
        .into_iter()
        .map(|(model_name, provider_name, credential_id, remaining_secs)| {
            serde_json::json!({
                "modelName": model_name,
                "providerName": provider_name,
                "credentialId": credential_id,
                "remainingSecs": remaining_secs,
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({
        "healthy": true,
        "models": {
            "total": tm,
            "active": am,
            "inactive": tm - am,
        },
        "providers": {
            "total": tp,
            "active": ap,
            "inactive": tp - ap,
        },
        "apiKeys": {
            "total": tk,
            "active": ak,
            "inactive": tk - ak,
        },
        "penalties": penalties_json,
    }))
}

pub async fn refresh_cache(state: web::Data<AppState>) -> HttpResponse {
    state.scheduler_cache.clear().await;
    state.api_key_cache.refresh(&state.database.api_keys).await;
    log::info!("Cache refreshed manually");
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

#[derive(Deserialize)]
pub struct UsageLogStatsQuery {
    pub group_by: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

pub async fn usage_log_stats(
    state: web::Data<AppState>,
    query: web::Query<UsageLogStatsQuery>,
) -> HttpResponse {
    const ALLOWED_GROUP_BY: &[&str] = &["provider", "model", "api_key", "day", "credential", "provider_model"];
    if !ALLOWED_GROUP_BY.contains(&query.group_by.as_str()) {
        return response::bad_request("unsupported group_by, must be one of: provider, model, api_key, day, credential, provider_model");
    }

    match state.database.usage_logs.query_stats(
        &query.group_by,
        query.start_time.as_deref(),
        query.end_time.as_deref(),
    )
    .await
    {
        Ok(resp) => HttpResponse::Ok().json(resp),
        Err(e) => {
            log::error!("Stats query error: {}", e);
            response::db_error()
        }
    }
}
