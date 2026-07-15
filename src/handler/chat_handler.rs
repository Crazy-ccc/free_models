use actix_web::{web, HttpResponse};
use log::info;
use serde_json::Value;

use crate::AppState;
use crate::error;
use crate::service::model_service;
use crate::service::proxy_service;

/// GET /v1/models - 返回启用的模型列表（OpenAI 格式）
pub async fn list_models(state: web::Data<AppState>) -> HttpResponse {
    match model_service::get_all_model_names(&state.db).await {
        Ok(names) => {
            let models: Vec<Value> = names
                .into_iter()
                .map(|name| {
                    let timestamp = chrono::Utc::now().timestamp();
                    serde_json::json!({
                        "id": name,
                        "object": "model",
                        "created": timestamp,
                        "owned_by": "model-proxy"
                    })
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "object": "list",
                "data": models
            }))
        }
        Err(e) => error::internal_error(&format!("Failed to list models: {}", e)),
    }
}

/// 公共请求处理：模型查询 → 协议筛选 → 前置 → 惩罚排序 → 转发
async fn handle_chat_request(
    state: web::Data<AppState>,
    body: web::Json<Value>,
    protocol: proxy_service::Protocol,
) -> HttpResponse {
    let model_name = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // 校验 messages 字段
    let messages_valid = body
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false);
    if !messages_valid {
        return protocol.bad_request("messages field is required");
    }

    let all_models = match model_service::get_all_available_models_by_priority(
        &state.db,
        &state.model_cache,
        &state.provider_cache,
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            return protocol.internal_error(&format!("Failed to query models: {}", e));
        }
    };

    let all_models: Vec<_> = all_models
        .into_iter()
        .filter(|m| m.supports_protocol(protocol.as_str()))
        .collect();

    if all_models.is_empty() {
        return protocol.service_unavailable("No available models");
    }

    let models = if !model_name.is_empty() {
        merge_preferred_first(all_models, model_name)
    } else {
        all_models
    };

    let models = state
        .priority_penalty
        .sort_penalized_last(models, |m| (m.model_name.clone(), m.provider_name.clone()));

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_stream {
        proxy_service::proxy_chat_completion_stream(&state.client, &body, &models, &state.priority_penalty, protocol)
            .await
            .unwrap_or_else(|response| response)
    } else {
        proxy_service::proxy_chat_completion(&state.client, &body, &models, &state.priority_penalty, protocol)
            .await
            .unwrap_or_else(|response| response)
    }
}

fn merge_preferred_first(
    all_models: Vec<model_service::ModelProviderInfo>,
    preferred_name: &str,
) -> Vec<model_service::ModelProviderInfo> {
    let (mut preferred, rest): (Vec<_>, Vec<_>) = all_models
        .into_iter()
        .partition(|m| m.model_name == preferred_name);
    preferred.extend(rest);
    preferred
}

/// POST /v1/chat/completions - 对话补全代理（OpenAI 协议）
pub async fn chat_completions(
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    handle_chat_request(state, body, proxy_service::Protocol::OpenAI).await
}

/// POST /v1/messages - Anthropic Messages API 代理
pub async fn anthropic_messages(
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    handle_chat_request(state, body, proxy_service::Protocol::Anthropic).await
}

/// GET /health - 健康检查（不需鉴权）
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

/// POST /admin/cache/refresh - 手动刷新模型/供应商缓存（需鉴权）
pub async fn refresh_cache(state: web::Data<AppState>) -> HttpResponse {
    state.model_cache.clear();
    state.provider_cache.clear();
    info!("Cache refreshed manually");
    HttpResponse::Ok().json(serde_json::json!({"status": "refreshed"}))
}
