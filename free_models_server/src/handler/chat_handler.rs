use actix_web::{HttpRequest, HttpResponse, web};
use log::warn;
use crate::db::impls::ApiKeyStoreSeaorm;
use serde_json::Value;

use crate::AppState;
use crate::response;
use crate::service::proxy_service;
use crate::util::model_scheduler;
use crate::util::tokenizer;

fn extract_bearer_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

async fn resolve_api_key(req: &HttpRequest, api_key_store: &ApiKeyStoreSeaorm) -> (Option<i32>, Option<String>) {
    match extract_bearer_token(req) {
        Some(token) => match api_key_store.find_by_key_value(&token).await {
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
    match model_scheduler::get_all_model_names(
        &state.database.model_configs,
        &state.database.provider_model_maps,
        &state.database.provider_credentials,
    ).await {
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
        Err(e) => response::internal_error(&format!("Failed to list models: {}", e)),
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
    let model_name = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let is_responses = protocol == proxy_service::Protocol::Responses;
    if let Err(resp) = validate_request_body(&body, protocol) {
        return resp;
    }
    let all_models = match model_scheduler::schedule_all_available(&state.database, &state.encryption_key, &state.scheduler_cache).await {
        Ok(m) => m,
        Err(e) => return protocol.internal_error(&format!("Failed to query models: {}", e)),
    };
    let all_models: Vec<_> = all_models.into_iter().filter(|m| m.supports_protocol(protocol.as_str())).collect();
    if all_models.is_empty() {
        return protocol.service_unavailable("No available models");
    }
    let models = if !model_name.is_empty() { merge_preferred_first(all_models, model_name) } else { all_models };
    let prompt_texts = extract_prompt_text(&body, is_responses);
    let estimated_prompt = prompt_texts.iter().map(|s| tokenizer::estimate_prompt_tokens(s)).sum::<usize>() as i32;
    let models = filter_by_context_window(models, estimated_prompt);
    if models.is_empty() {
        return protocol.bad_request(&format!("Prompt too long (estimated {} tokens), exceeds all available models' context window", estimated_prompt));
    }
    let is_stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let api_key_ctx = proxy_service::ApiKeyContext { id: api_key_id, name: api_key_name };
    let forward_meta = proxy_service::ForwardMeta { protocol, is_stream, start_time };
    proxy_service::proxy_chat_completion_inner(&state.client, &body, &models, &state.priority_penalty, &state.cache_affinity, api_key_ctx, forward_meta, &state.database.provider_credentials, &state.ssrf_checker)
        .await
        .unwrap_or_else(|response| response)
}

fn merge_preferred_first(
    all_models: Vec<model_scheduler::ModelScheduleInfo>,
    preferred_name: &str,
) -> Vec<model_scheduler::ModelScheduleInfo> {
    let (mut preferred, rest): (Vec<_>, Vec<_>) = all_models
        .into_iter()
        .partition(|m| m.model_name == preferred_name);
    preferred.extend(rest);
    preferred
}

/// 校验请求体：Responses 协议校验 input 字段，其余协议校验 messages 字段。
fn validate_request_body(body: &Value, protocol: proxy_service::Protocol) -> Result<(), HttpResponse> {
    let is_responses = protocol == proxy_service::Protocol::Responses;
    if is_responses {
        let input_valid = match body.get("input") {
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Array(arr)) => !arr.is_empty(),
            _ => false,
        };
        if !input_valid {
            return Err(protocol.bad_request("input field is required"));
        }
    } else {
        let messages_valid = body
            .get("messages")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);
        if !messages_valid {
            return Err(protocol.bad_request("messages field is required"));
        }
    }
    Ok(())
}

/// 提取 prompt 文本用于 token 估算（纯函数，无副作用）。
fn extract_prompt_text(body: &Value, is_responses: bool) -> Vec<String> {
    if is_responses {
        match body.get("input") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            item.get("content")
                                .and_then(|c| c.as_str())
                                .map(|s| s.to_string())
                        })
                })
                .collect(),
            _ => vec![],
        }
    } else {
        body.get("messages")
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
            .unwrap_or_default()
    }
}

/// 过滤掉上下文窗口不足以容纳估算 prompt 的模型。
/// 模型级判断：任一映射的 context_length 足够即保留该模型。
fn filter_by_context_window(
    models: Vec<model_scheduler::ModelScheduleInfo>,
    estimated_prompt: i32,
) -> Vec<model_scheduler::ModelScheduleInfo> {
    models
        .into_iter()
        .filter(|m| {
            m.maps
                .iter()
                .any(|map| estimated_prompt <= map.context_length)
        })
        .collect()
}

/// 解析 Bearer token → 调用 handle_chat_request
async fn dispatch_chat(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
    protocol: proxy_service::Protocol,
) -> HttpResponse {
    let (api_key_id, api_key_name) = resolve_api_key(&req, &state.database.api_keys).await;
    handle_chat_request(state, body, protocol, api_key_id, api_key_name).await
}

/// POST /v1/chat/completions - 对话补全代理（OpenAI 协议）
pub async fn chat_completions(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    dispatch_chat(req, state, body, proxy_service::Protocol::OpenAI).await
}

/// POST /v1/messages - Anthropic Messages API 代理
pub async fn anthropic_messages(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    dispatch_chat(req, state, body, proxy_service::Protocol::Anthropic).await
}

/// POST /v1/responses - OpenAI Responses API 代理
pub async fn responses_messages(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> HttpResponse {
    dispatch_chat(req, state, body, proxy_service::Protocol::Responses).await
}

/// GET /health - 健康检查（不需鉴权）
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}


