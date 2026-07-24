use actix_web::http::StatusCode;
use actix_web::web;
use actix_web::HttpResponse;
use futures_util::StreamExt;
use log::{error, info, warn};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use sea_orm::DatabaseConnection;

use crate::error;
use crate::service::model_service::ModelProviderInfo;
use crate::service::usage_log_service;
use crate::util::penalty::PriorityPenalty;

/// 支持的协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAI,
    Anthropic,
}

impl Protocol {
    /// 返回上游 API 路径（不含 base_url）
    pub fn path(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "/chat/completions",
            Protocol::Anthropic => "/messages",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "openai",
            Protocol::Anthropic => "anthropic",
        }
    }

    pub fn internal_error(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => error::internal_error(message),
            Protocol::Anthropic => error::anthropic_internal_error(message),
        }
    }

    pub fn service_unavailable(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => error::service_unavailable(message),
            Protocol::Anthropic => error::anthropic_service_unavailable(message),
        }
    }

    pub fn bad_request(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => error::openai_error(
                StatusCode::BAD_REQUEST,
                message,
                "invalid_request_error",
            ),
            Protocol::Anthropic => error::anthropic_bad_request(message),
        }
    }

    pub fn extract_usage(&self, usage: &Value) -> UsageInfo {
        fn get_i64(v: &Value, key: &str) -> i64 {
            v.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
        }

        match self {
            Protocol::OpenAI => {
                let prompt = get_i64(usage, "prompt_tokens") as i32;
                let completion = get_i64(usage, "completion_tokens") as i32;
                let total = get_i64(usage, "total_tokens") as i32;
                let cached = usage
                    .get("prompt_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                UsageInfo {
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    total_tokens: total,
                    cache_hit_tokens: cached,
                    cache_miss_tokens: prompt - cached,
                }
            }
            Protocol::Anthropic => {
                let input = get_i64(usage, "input_tokens") as i32;
                let output = get_i64(usage, "output_tokens") as i32;
                let cache_read = get_i64(usage, "cache_read_input_tokens") as i32;
                UsageInfo {
                    prompt_tokens: input,
                    completion_tokens: output,
                    total_tokens: input + output,
                    cache_hit_tokens: cache_read,
                    cache_miss_tokens: input,
                }
            }
        }
    }
}

/// 从上游响应的 usage 对象中提取 token 指标，兼容 OpenAI 和 Anthropic 两种格式
pub struct UsageInfo {
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    cache_hit_tokens: i32,
    cache_miss_tokens: i32,
}

fn spawn_usage_log(
    db: DatabaseConnection,
    model_info: &ModelProviderInfo,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    protocol: Protocol,
    duration_ms: i32,
    is_stream: bool,
    info: UsageInfo,
    status: &str,
    error_message: Option<&str>,
) {
    let model_name = model_info.model_name.clone();
    let provider_name = model_info.provider_name.clone();
    let model_config_id = model_info.model_config_id;
    let provider_config_id = model_info.provider_config_id;
    let provider_credential_id = model_info.provider_credential_id;
    let api_key_name = api_key_name.map(|s| s.to_string());
    let protocol_str = protocol.as_str().to_string();
    let status = status.to_string();
    let error_message = error_message.map(|s| s.to_string());
    actix_web::rt::spawn(async move {
        if let Err(e) = usage_log_service::create(
            &db,
            api_key_id,
            api_key_name.as_deref(),
            Some(model_config_id),
            Some(provider_config_id),
            Some(provider_credential_id),
            &model_name,
            &provider_name,
            &protocol_str,
            &status,
            error_message.as_deref(),
            info.prompt_tokens,
            info.completion_tokens,
            info.total_tokens,
            info.cache_hit_tokens,
            info.cache_miss_tokens,
            duration_ms,
            is_stream,
        ).await {
            log::error!("Failed to persist usage log: {}", e);
        }
    });
}

fn try_parse_usage_from_sse_chunk(bytes: &[u8], protocol: Protocol) -> Option<UsageInfo> {
    if !bytes.windows(7).any(|w| w == b"\"usage\"") {
        return None;
    }
    let s = std::str::from_utf8(bytes).ok()?;
    for line in s.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<Value>(data) {
                if json.get("usage").is_some() {
                    return Some(protocol.extract_usage(json.get("usage")?));
                }
            }
        }
    }
    None
}

/// 上游转发结果：区分"成功""需重试"两种情况
enum ForwardOutcome {
    /// 上游返回 2xx，携带响应供调用方构造最终回复
    Success(reqwest::Response),
    /// 上游返回 4xx 5xx 或网络错误，应跳过并尝试下一个 provider
    Retry,
}

/// OpenAI SSE 流中段出错时，下发一个干净的结束帧
fn sse_close_event() -> web::Bytes {
    web::Bytes::from_static(b"data: [DONE]\n\n")
}

/// Anthropic SSE 流中段出错时，下发 Anthropic 格式的错误事件
fn anthropic_sse_close_event() -> web::Bytes {
    web::Bytes::from_static(
        b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"upstream stream interrupted\"}}\n\n",
    )
}

/// 根据协议返回对应的 SSE 关闭帧
fn sse_close_event_for(protocol: Protocol) -> web::Bytes {
    match protocol {
        Protocol::OpenAI => sse_close_event(),
        Protocol::Anthropic => anthropic_sse_close_event(),
    }
}

/// 向单个 provider 发送请求并做初步状态码判断。
///
/// 公共的"构建 URL + 设置 header + 发送请求 + 4xx/5xx/网络错误分流"逻辑都收敛于此，
/// 两个公开函数（流式/非流式）仅负责把 `Success` 响应转换成对应格式。
async fn forward_to_provider(
    client: &Client,
    model_info: &ModelProviderInfo,
    body: &Value,
    is_stream: bool,
    protocol: Protocol,
) -> ForwardOutcome {
    let url = format!(
        "{}{}",
        model_info.base_url.trim_end_matches('/'),
        protocol.path()
    );

    let mut request_body = body.clone();
    request_body["model"] = Value::String(model_info.model_id.clone());
    if is_stream {
        request_body["stream"] = Value::Bool(true);
    }

    let mut request = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "identity")
        .json(&request_body)
        .timeout(Duration::from_secs(model_info.timeout));

    // 按协议设置认证头
    request = match protocol {
        Protocol::OpenAI => request.header("Authorization", format!("Bearer {}", model_info.api_key)),
        Protocol::Anthropic => request
            .header("x-api-key", &model_info.api_key)
            .header("anthropic-version", "2023-06-01"),
    };

    let response = request.send().await;

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                return ForwardOutcome::Success(resp);
            }
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();
            warn!(
                "Provider {} returned error status: {}, error: {}",
                model_info.provider_name, status, error_text
            );
            ForwardOutcome::Retry
        }
        Err(e) => {
            if e.is_timeout() {
                warn!(
                    "Request to provider {} timed out: {}",
                    model_info.provider_name, e
                );
            } else {
                warn!(
                    "Request to provider {} failed: {}",
                    model_info.provider_name, e
                );
            }
            ForwardOutcome::Retry
        }
    }
}

/// 所有 provider 均不可用时的统一 503 响应（按协议返回对应格式）
fn all_models_unavailable(models: &[ModelProviderInfo], protocol: Protocol) -> HttpResponse {
    let model_name = models
        .first()
        .map(|m| m.model_name.as_str())
        .unwrap_or("unknown");
    let message = format!("All models unavailable for: {}", model_name);
    protocol.service_unavailable(&message)
}

async fn handle_stream_response(
    resp: reqwest::Response,
    model_info: &ModelProviderInfo,
    db: &DatabaseConnection,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> HttpResponse {
    let byte_stream = resp.bytes_stream();
    let close_event = sse_close_event_for(protocol);
    let model_info = model_info.clone();
    let db_clone = db.clone();
    let api_key_id_clone = api_key_id;
    let api_key_name_clone = api_key_name.map(|s| s.to_string());
    let start_time_clone = start_time;

    let stream = byte_stream.map(move |item| {
        match item {
            Ok(bytes) => {
                if let Some(info) = try_parse_usage_from_sse_chunk(&bytes, protocol) {
                    info!(
                        "Model {} via provider {} stream usage: prompt_tokens={}, completion_tokens={}, total_tokens={}",
                        model_info.model_name, model_info.provider_name, info.prompt_tokens, info.completion_tokens, info.total_tokens
                    );
                    spawn_usage_log(
                        db_clone.clone(),
                        &model_info,
                        api_key_id_clone,
                        api_key_name_clone.as_deref(),
                        protocol,
                        start_time_clone.elapsed().as_millis() as i32,
                        true,
                        info,
                        "success",
                        None,
                    );
                }
                Ok::<_, actix_web::Error>(web::Bytes::from(bytes.to_vec()))
            }
            Err(_) => Ok::<_, actix_web::Error>(close_event.clone()),
        }
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(stream)
}

async fn handle_non_stream_response(
    resp: reqwest::Response,
    model_info: &ModelProviderInfo,
    db: &DatabaseConnection,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::OK);

    let body_bytes = resp.bytes().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        protocol.internal_error("Failed to read upstream response")
    })?;

    let duration_ms = start_time.elapsed().as_millis() as i32;

    if let Ok(json_body) = serde_json::from_slice::<Value>(&body_bytes) {
        if let Some(usage) = json_body.get("usage") {
            info!(
                "Model {} via provider {} usage: {}",
                model_info.model_name, model_info.provider_name, usage
            );
            let info = protocol.extract_usage(&usage);
            spawn_usage_log(
                db.clone(),
                model_info,
                api_key_id,
                api_key_name,
                protocol,
                duration_ms,
                false,
                info,
                "success",
                None,
            );
        }
    }

    Ok(HttpResponse::build(status)
        .content_type("application/json")
        .body(body_bytes.to_vec()))
}

/// 公共重试循环：遍历模型、转发、惩罚失败模型、全部失败返回 503。
/// `is_stream` 控制转发模式与成功响应的构建方式。
async fn proxy_chat_completion_inner(
    client: &Client,
    db: &DatabaseConnection,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
    is_stream: bool,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    let mut last_model_info: Option<&ModelProviderInfo> = None;

    for model_info in models {
        last_model_info = Some(model_info);
        info!(
            "Trying {}model: {} via provider: {} (priority: {}, protocol: {})",
            if is_stream { "streaming " } else { "" },
            model_info.model_name, model_info.provider_name, model_info.priority, protocol.as_str()
        );

        match forward_to_provider(client, model_info, body, is_stream, protocol).await {
            ForwardOutcome::Success(resp) => {
                if is_stream {
                    return Ok(handle_stream_response(resp, model_info, db, api_key_id, api_key_name, protocol, start_time).await);
                } else {
                    return handle_non_stream_response(resp, model_info, db, api_key_id, api_key_name, protocol, start_time).await;
                }
            }
            ForwardOutcome::Retry => {
                penalty.penalize(&model_info.model_name, &model_info.provider_name).await;
                warn!("Model {} via provider {} failed, penalized for {} seconds", model_info.model_name, model_info.provider_name, penalty.ttl_secs());
                continue;
            }
        }
    }

    let duration_ms = start_time.elapsed().as_millis() as i32;
    if let Some(last_info) = last_model_info {
        spawn_usage_log(
            db.clone(),
            last_info,
            api_key_id,
            api_key_name,
            protocol,
            duration_ms,
            is_stream,
            UsageInfo {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cache_hit_tokens: 0,
                cache_miss_tokens: 0,
            },
            "failed",
            Some("All providers failed"),
        );
    }

    Err(all_models_unavailable(models, protocol))
}

/// 非流式代理转发（带故障切换）
pub async fn proxy_chat_completion(
    client: &Client,
    db: &DatabaseConnection,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    proxy_chat_completion_inner(
        client,
        db,
        body,
        models,
        penalty,
        protocol,
        false,
        api_key_id,
        api_key_name,
        start_time,
    )
    .await
}

/// 流式代理转发（SSE，带故障切换）
pub async fn proxy_chat_completion_stream(
    client: &Client,
    db: &DatabaseConnection,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    proxy_chat_completion_inner(
        client,
        db,
        body,
        models,
        penalty,
        protocol,
        true,
        api_key_id,
        api_key_name,
        start_time,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_close_event_is_done_frame() {
        assert_eq!(sse_close_event().as_ref(), b"data: [DONE]\n\n");
    }
}
