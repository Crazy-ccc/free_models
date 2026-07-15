use actix_web::http::StatusCode;
use actix_web::web;
use actix_web::HttpResponse;
use futures_util::StreamExt;
use log::{error, info, warn};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::error;
use crate::service::model_service::ModelProviderInfo;
use crate::service::penalty::PriorityPenalty;

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
                actix_web::http::StatusCode::BAD_REQUEST,
                message,
                "invalid_request_error",
            ),
            Protocol::Anthropic => error::anthropic_bad_request(message),
        }
    }
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

/// 公共重试循环：遍历模型、转发、惩罚失败模型、全部失败返回 503。
/// `is_stream` 控制转发模式与成功响应的构建方式。
async fn try_models_with_failover(
    client: &Client,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
    is_stream: bool,
) -> Result<HttpResponse, HttpResponse> {
    for model_info in models {
        info!(
            "Trying {}model: {} via provider: {} (priority: {}, protocol: {})",
            if is_stream { "streaming " } else { "" },
            model_info.model_name, model_info.provider_name, model_info.priority, protocol.as_str()
        );

        match forward_to_provider(client, model_info, body, is_stream, protocol).await {
            ForwardOutcome::Success(resp) => {
                if is_stream {
                    let byte_stream = resp.bytes_stream();
                    let close_event = sse_close_event_for(protocol);

                    // 上游中段出错时不抛 server error（响应头已发出会直接断连导致客户端 SSE stream error），
                    // 而是下发一个干净的结束帧，让客户端正常收尾。
                    let stream = byte_stream.map(move |item| match item {
                        Ok(bytes) => Ok::<_, actix_web::Error>(web::Bytes::from(bytes.to_vec())),
                        Err(_) => Ok::<_, actix_web::Error>(close_event.clone()),
                    });

                    return Ok(HttpResponse::Ok()
                        .content_type("text/event-stream")
                        .streaming(stream));
                } else {
                    let status = StatusCode::from_u16(resp.status().as_u16())
                        .unwrap_or(StatusCode::OK);

                    let body_bytes = resp.bytes().await.map_err(|e| {
                        error!("Failed to read response body: {}", e);
                        protocol.internal_error("Failed to read upstream response")
                    })?;

                    return Ok(HttpResponse::build(status)
                        .content_type("application/json")
                        .body(body_bytes.to_vec()));
                }
            }
            ForwardOutcome::Retry => {
                penalty.penalize(&model_info.model_name, &model_info.provider_name);
                warn!("Model {} via provider {} failed, penalized for {} seconds", model_info.model_name, model_info.provider_name, penalty.ttl_secs());
                continue;
            }
        }
    }

    Err(all_models_unavailable(models, protocol))
}

/// 非流式代理转发（带故障切换）
pub async fn proxy_chat_completion(
    client: &Client,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
) -> Result<HttpResponse, HttpResponse> {
    try_models_with_failover(client, body, models, penalty, protocol, false).await
}

/// 流式代理转发（SSE，带故障切换）
pub async fn proxy_chat_completion_stream(
    client: &Client,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
    protocol: Protocol,
) -> Result<HttpResponse, HttpResponse> {
    try_models_with_failover(client, body, models, penalty, protocol, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_close_event_is_done_frame() {
        assert_eq!(sse_close_event().as_ref(), b"data: [DONE]\n\n");
    }
}
