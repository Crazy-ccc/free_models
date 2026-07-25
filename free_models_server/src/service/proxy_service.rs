use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;
use log::{debug, error, info, warn};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::util::model_scheduler::ModelProviderInfo;
use crate::util::penalty::PriorityPenalty;
pub(crate) use crate::util::proxy_ssrf::validate_url_safe;
pub(crate) use crate::util::proxy_types::{Protocol, UsageInfo, log_status};
use crate::util::usage_log_collector::spawn_usage_log;

/// 扫描缓冲中的完整 SSE 事件，按 \n\n 分割提取，检测 usage 字段。
fn extract_usage_from_buffer(buffer: &[u8], protocol: Protocol) -> Option<UsageInfo> {
    let s = std::str::from_utf8(buffer).ok()?;
    for event in s.split("\n\n") {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(json) = serde_json::from_str::<Value>(data)
                && let Some(usage) = json.get("usage")
                && !usage.is_null() {
                return Some(protocol.extract_usage(usage));
            }
        }
    }
    None
}

/// 上游转发结果：区分"成功""需重试"两种情况
enum ForwardOutcome {
    Success(reqwest::Response),
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

    if let Err(e) = validate_url_safe(&url).await {
        warn!("SSRF check failed for {}: {}", url, e);
        return ForwardOutcome::Retry;
    }

    let mut request_body = body.clone();
    request_body["model"] = Value::String(model_info.model_id.clone());
    if is_stream {
        request_body["stream"] = Value::Bool(true);
        if protocol == Protocol::OpenAI {
            request_body["stream_options"] = serde_json::json!({"include_usage": true});
        }
    }

    let mut request = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "identity")
        .json(&request_body)
        .timeout(Duration::from_secs(model_info.timeout));

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
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> HttpResponse {
    use futures_util::StreamExt;

    let close_event = sse_close_event_for(protocol);
    let model_info = model_info.clone();
    let api_key_name = api_key_name.map(|s| s.to_string());

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<web::Bytes, actix_web::Error>>();

    actix_web::rt::spawn(async move {
        let mut upstream_stream = resp.bytes_stream();
        let mut buffer = Vec::new();
        let mut logged_usage = false;

        while let Some(item) = upstream_stream.next().await {
            match item {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);

                    if !logged_usage {
                        if let Some(info) = extract_usage_from_buffer(&buffer, protocol) {
                            debug!(
                                "Model {} via provider {} stream usage: prompt_tokens={}, completion_tokens={}, total_tokens={}",
                                model_info.model_name, model_info.provider_name,
                                info.prompt_tokens, info.completion_tokens, info.total_tokens
                            );
                            spawn_usage_log(
                                &model_info,
                                api_key_id,
                                api_key_name.as_deref(),
                                protocol,
                                start_time.elapsed().as_millis() as i32,
                                true,
                                info,
                                log_status::SUCCESS,
                                None,
                            );
                            logged_usage = true;
                        }
                    }

                    if tx.send(Ok(bytes)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(Ok(close_event.clone()));
                    break;
                }
            }
        }

        if !logged_usage {
            if let Some(info) = extract_usage_from_buffer(&buffer, protocol) {
                spawn_usage_log(
                    &model_info,
                    api_key_id,
                    api_key_name.as_deref(),
                    protocol,
                    start_time.elapsed().as_millis() as i32,
                    true,
                    info,
                    log_status::SUCCESS,
                    None,
                );
            }
        }
    });

    let rx_stream = futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(item) => Some((item, rx)),
            None => None,
        }
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(rx_stream)
}

async fn handle_non_stream_response(
    resp: reqwest::Response,
    model_info: &ModelProviderInfo,
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
            debug!(
                "Model {} via provider {} usage: {}",
                model_info.model_name, model_info.provider_name, usage
            );
            let info = protocol.extract_usage(&usage);
            spawn_usage_log(
                model_info,
                api_key_id,
                api_key_name,
                protocol,
                duration_ms,
                false,
                info,
                log_status::SUCCESS,
                None,
            );
        } else {
            spawn_usage_log(
                model_info,
                api_key_id,
                api_key_name,
                protocol,
                duration_ms,
                false,
                UsageInfo {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    cache_hit_tokens: 0,
                    cache_miss_tokens: 0,
                },
                log_status::SUCCESS,
                None,
            );
        }
    }

    Ok(HttpResponse::build(status)
        .content_type("application/json")
        .body(body_bytes.to_vec()))
}

/// 公共重试循环：遍历模型、转发、惩罚失败模型、全部失败返回 503。
async fn proxy_chat_completion_inner(
    client: &Client,
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
                return if is_stream {
                    Ok(handle_stream_response(resp, model_info, api_key_id, api_key_name, protocol, start_time).await)
                } else {
                    handle_non_stream_response(resp, model_info, api_key_id, api_key_name, protocol, start_time).await
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
            log_status::FAILED,
            Some("All providers failed"),
        );
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
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    proxy_chat_completion_inner(
        client,
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
