use actix_web::{web, HttpRequest, HttpResponse};
use log::warn;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::AppState;
use crate::error;
use crate::service::api_key_service;
use crate::service::model_service;
use crate::service::proxy_service;
use crate::util::tokenizer;

fn extract_bearer_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

async fn resolve_api_key(req: &HttpRequest, db: &DatabaseConnection) -> (Option<i32>, Option<String>) {
    match extract_bearer_token(req) {
        Some(token) => match api_key_service::get_by_key_value(db, &token).await {
            Ok(Some(key)) => (Some(key.id), Some(key.name)),
            Ok(None) => {
                warn!("API key not found in db: {}", token);
                (None, None)
            }
            Err(e) => {
                warn!("Failed to query API key: {}", e);
                (None, None)
            }
        },
        None => (None, None),
    }
}

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
    api_key_id: Option<i32>,
    api_key_name: Option<String>,
) -> HttpResponse {
    let start_time = std::time::Instant::now();
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
        &state.encryption_key,
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
        .sort_penalized_last(models, |m| (m.model_name.clone(), m.provider_name.clone()))
        .await;

    // 估算 prompt token 数，过滤掉上下文窗口不足的模型
    let prompt_text: Vec<String> = body
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    msg.get("content")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let estimated_prompt = prompt_text.iter().map(|s| tokenizer::estimate_prompt_tokens(s)).sum::<usize>() as i32;

    let models: Vec<_> = models
        .into_iter()
        .filter(|m| estimated_prompt <= m.context_length)
        .collect();

    if models.is_empty() {
        return protocol.bad_request(
            &format!("Prompt too long (estimated {} tokens), exceeds all available models' context window", estimated_prompt)
        );
    }

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_stream {
        proxy_service::proxy_chat_completion_stream(
            &state.client,
            &state.db,
            &body,
            &models,
            &state.priority_penalty,
            protocol,
            api_key_id,
            api_key_name.as_deref(),
            start_time,
        )
        .await
        .unwrap_or_else(|response| response)
    } else {
        proxy_service::proxy_chat_completion(
            &state.client,
            &state.db,
            &body,
            &models,
            &state.priority_penalty,
            protocol,
            api_key_id,
            api_key_name.as_deref(),
            start_time,
        )
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
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    let (api_key_id, api_key_name) = resolve_api_key(&req, &state.db).await;
    handle_chat_request(state, body, proxy_service::Protocol::OpenAI, api_key_id, api_key_name).await
}

/// POST /v1/messages - Anthropic Messages API 代理
pub async fn anthropic_messages(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    let (api_key_id, api_key_name) = resolve_api_key(&req, &state.db).await;
    handle_chat_request(state, body, proxy_service::Protocol::Anthropic, api_key_id, api_key_name).await
}

/// GET /health - 健康检查（不需鉴权）
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}


