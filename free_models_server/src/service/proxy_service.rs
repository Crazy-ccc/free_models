use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;
use log::{debug, error, info, warn};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::util::cache_affinity::CacheAffinity;
use crate::util::model_scheduler::{CredentialInfo, ModelProviderMap, ModelScheduleInfo};
pub(crate) use crate::util::penalty::CircuitBreaker;
pub(crate) use crate::util::proxy_ssrf::validate_url_safe;
pub(crate) use crate::util::proxy_types::{ApiKeyContext, ForwardMeta, LogContext, Protocol, UsageInfo, log_status};
use crate::util::usage_log_collector::spawn_usage_log;

pub(crate) fn join_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

fn log_stream_usage(
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    api_key_ctx: &ApiKeyContext,
    protocol: Protocol,
    start_time: std::time::Instant,
    info: UsageInfo,
) {
    let log_ctx = LogContext {
        protocol,
        duration_ms: start_time.elapsed().as_millis() as i32,
        is_stream: true,
        info,
        status: log_status::SUCCESS.to_string(),
        error_message: None,
    };
    spawn_usage_log(model_info, map, cred, api_key_ctx, &log_ctx);
}

fn extract_usage_from_buffer(buffer: &[u8], protocol: Protocol) -> Option<UsageInfo> {
    let s = std::str::from_utf8(buffer).ok()?;
    let find_usage_in_json = |json: &Value| -> Option<UsageInfo> {
        if let Some(usage) = json.get("usage") {
            if !usage.is_null() {
                return Some(protocol.extract_usage(usage));
            }
        }
        if protocol == Protocol::Responses {
            if let Some(response) = json.get("response") {
                if let Some(usage) = response.get("usage") {
                    if !usage.is_null() {
                        return Some(protocol.extract_usage(usage));
                    }
                }
            }
        }
        None
    };
    for event in s.split("\n\n") {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(json) = serde_json::from_str::<Value>(data)
            {
                if let Some(info) = find_usage_in_json(&json) {
                    return Some(info);
                }
            }
        }
    }
    None
}

enum ForwardOutcome {
    Success(reqwest::Response),
    Retry,
    Fail(String),
}

fn sse_close_event() -> web::Bytes {
    web::Bytes::from_static(b"data: [DONE]\n\n")
}

fn anthropic_sse_close_event() -> web::Bytes {
    web::Bytes::from_static(
        b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"upstream stream interrupted\"}}\n\n",
    )
}

fn sse_close_event_for(protocol: Protocol) -> web::Bytes {
    match protocol {
        Protocol::OpenAI => sse_close_event(),
        Protocol::Anthropic | Protocol::Responses => anthropic_sse_close_event(),
    }
}

async fn record_success_metrics(
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    circuit_breaker: &CircuitBreaker,
    cache_affinity: &CacheAffinity,
    api_key_id: Option<i32>,
) {
    circuit_breaker.record_success(&model_info.model_name, &map.provider_name, cred.provider_credential_id);
    if let Some(api_key_id_value) = api_key_id {
        cache_affinity.record_success(
            api_key_id_value,
            &model_info.model_name,
            &map.provider_name,
            cred.provider_credential_id,
        );
    }
}

async fn handle_success(
    resp: reqwest::Response,
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    circuit_breaker: &CircuitBreaker,
    cache_affinity: &CacheAffinity,
    api_key_ctx: &ApiKeyContext,
    forward_meta: ForwardMeta,
) -> Result<HttpResponse, HttpResponse> {
    record_success_metrics(model_info, map, cred, circuit_breaker, cache_affinity, api_key_ctx.id).await;
    if forward_meta.is_stream {
        Ok(handle_stream_response(resp, model_info, map, cred, api_key_ctx, forward_meta.protocol, forward_meta.start_time).await)
    } else {
        handle_non_stream_response(resp, model_info, map, cred, api_key_ctx, forward_meta.protocol, forward_meta.start_time).await
    }
}

async fn forward_to_provider(
    client: &Client,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    body: &Value,
    is_stream: bool,
    protocol: Protocol,
) -> ForwardOutcome {
    let url = join_url(&map.base_url, protocol.path());

    if let Err(e) = validate_url_safe(&url).await {
        warn!("SSRF check failed for {}: {}", url, e);
        return ForwardOutcome::Fail(format!("URL validation failed for {}", map.provider_name));
    }

    let mut request_body = body.clone();
    request_body["model"] = Value::String(map.model_id.clone());
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
        .timeout(Duration::from_secs(map.timeout));

    request = match protocol {
        Protocol::OpenAI => request.header("Authorization", format!("Bearer {}", cred.api_key)),
        Protocol::Anthropic => request
            .header("x-api-key", &cred.api_key)
            .header("anthropic-version", "2023-06-01"),
        Protocol::Responses => request.header("Authorization", format!("Bearer {}", cred.api_key)),
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
                    map.provider_name, status, error_text
                );
                if [429, 500, 502, 503, 504].contains(&status.as_u16()) {
                    ForwardOutcome::Retry
                } else {
                    ForwardOutcome::Fail(error_text)
                }
            }
        Err(e) => {
            if e.is_timeout() {
                warn!(
                    "Request to provider {} timed out: {}",
                    map.provider_name, e
                );
            } else {
                warn!(
                    "Request to provider {} failed: {}",
                    map.provider_name, e
                );
            }
            ForwardOutcome::Retry

        }
    }
}

async fn handle_stream_response(
    resp: reqwest::Response,
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    api_key_ctx: &ApiKeyContext,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> HttpResponse {
    use futures_util::StreamExt;

    let close_event = sse_close_event_for(protocol);
    let model_info = model_info.clone();
    let map = map.clone();
    let cred = cred.clone();
    let api_key_ctx = api_key_ctx.clone();

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
                                model_info.model_name, map.provider_name,
                                info.prompt_tokens, info.completion_tokens, info.total_tokens
                            );
                            log_stream_usage(
                                &model_info, &map, &cred,
                                &api_key_ctx,
                                protocol,
                                start_time,
                                info,
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
                log_stream_usage(
                    &model_info, &map, &cred,
                    &api_key_ctx,
                    protocol,
                    start_time,
                    info,
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
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    api_key_ctx: &ApiKeyContext,
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

    let info = if let Ok(json_body) = serde_json::from_slice::<Value>(&body_bytes) {
        json_body
            .get("usage")
            .filter(|u| !u.is_null())
            .map(|u| {
                debug!(
                    "Model {} via provider {} usage: {}",
                    model_info.model_name, map.provider_name, u
                );
                protocol.extract_usage(u)
            })
            .unwrap_or_default()
    } else {
        UsageInfo::default()
    };
    let log_ctx = LogContext {
        protocol,
        duration_ms,
        is_stream: false,
        info,
        status: log_status::SUCCESS.to_string(),
        error_message: None,
    };
    spawn_usage_log(model_info, map, cred, api_key_ctx, &log_ctx);

    Ok(HttpResponse::build(status)
        .content_type("application/json")
        .body(body_bytes.to_vec()))
}

fn reorder_by_affinity(
    models: &[ModelScheduleInfo],
    api_key_id: Option<i32>,
    cache_affinity: &CacheAffinity,
) -> Vec<ModelScheduleInfo> {
    let Some(api_key_id_value) = api_key_id else {
        return models.to_vec();
    };
    let Some(first) = models.first() else {
        return Vec::new();
    };

    let model_name = first.model_name.as_str();
    let mut result: Vec<ModelScheduleInfo> = models.to_vec();

    if let Some(entry) = cache_affinity.get_affinity(api_key_id_value, model_name) {
        for model in &mut result {
            let mut affinity_map: Option<ModelProviderMap> = None;
            let mut same_provider: Vec<ModelProviderMap> = Vec::new();
            let mut rest: Vec<ModelProviderMap> = Vec::new();

            for map in model.maps.drain(..) {
                if map.provider_name == entry.provider_name {
                    if map.credentials.iter().any(|c| c.provider_credential_id == entry.provider_credential_id) {
                        affinity_map = Some(map);
                    } else {
                        same_provider.push(map);
                    }
                } else {
                    rest.push(map);
                }
            }
            model.maps.clear();
            if let Some(m) = affinity_map {
                model.maps.push(m);
            }
            model.maps.extend(same_provider);
            model.maps.extend(rest);
        }
    } else {
        for model in &mut result {
            for map in &mut model.maps {
                if map.credentials.len() > 1 {
                    map.credentials.sort_by_key(|c| {
                        let count = cache_affinity.count_by_credential(c.provider_credential_id);
                        (count, c.provider_credential_id)
                    });
                }
            }
        }
    }

    result
}

pub(crate) async fn proxy_chat_completion_inner(
    client: &Client,
    body: &Value,
    models: &[ModelScheduleInfo],
    circuit_breaker: &web::Data<CircuitBreaker>,
    cache_affinity: &CacheAffinity,
    api_key_ctx: ApiKeyContext,
    forward_meta: ForwardMeta,
) -> Result<HttpResponse, HttpResponse> {
    let ordered_models = reorder_by_affinity(models, api_key_ctx.id, cache_affinity);
    let mut last_log_info: Option<(&ModelScheduleInfo, &ModelProviderMap, &CredentialInfo)> = None;
    let mut error_details: Vec<String> = Vec::new();

    for model in &ordered_models {
        for map in &model.maps {
            for cred in &map.credentials {
                let (allowed, state) = circuit_breaker.is_allowed(&model.model_name, &map.provider_name, cred.provider_credential_id);
                if !allowed {
                    info!(
                        "Skipping model {} via provider {} credential {} (circuit breaker state: {:?})",
                        model.model_name, map.provider_name, cred.provider_credential_id, state
                    );
                    error_details.push(format!("{} via {} credential {}: circuit breaker blocked ({:?})", model.model_name, map.provider_name, cred.provider_credential_id, state));
                    continue;
                }

                last_log_info = Some((model, map, cred));
                info!(
                    "Trying {}model: {} via provider: {} credential: {} (priority: {}, protocol: {})",
                    if forward_meta.is_stream { "streaming " } else { "" },
                    model.model_name, map.provider_name, cred.provider_credential_id, model.priority, forward_meta.protocol.as_str()
                );

                match forward_to_provider(client, map, cred, body, forward_meta.is_stream, forward_meta.protocol).await {
                    ForwardOutcome::Success(resp) => {
                        return handle_success(resp, model, map, cred, circuit_breaker, cache_affinity, &api_key_ctx, forward_meta).await;
                    }
                    ForwardOutcome::Retry => {
                        warn!("Model {} via provider {} credential {} returned retryable error, retrying after 500ms...", model.model_name, map.provider_name, cred.provider_credential_id);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        match forward_to_provider(client, map, cred, body, forward_meta.is_stream, forward_meta.protocol).await {
                            ForwardOutcome::Success(resp) => {
                                return handle_success(resp, model, map, cred, circuit_breaker, cache_affinity, &api_key_ctx, forward_meta).await;
                            }
                            ForwardOutcome::Retry | ForwardOutcome::Fail(_) => {
                                circuit_breaker.record_failure(&model.model_name, &map.provider_name, cred.provider_credential_id);
                                warn!("Model {} via provider {} credential {} failed after retry, circuit breaker updated", model.model_name, map.provider_name, cred.provider_credential_id);
                                error_details.push(format!("{} via {} credential {}: failed after retry", model.model_name, map.provider_name, cred.provider_credential_id));
                                continue;
                            }
                        }
                    }
                    ForwardOutcome::Fail(error_text) => {
                        warn!("Model {} via provider {} returned non-retryable error: {}", model.model_name, map.provider_name, error_text);
                        let duration_ms = forward_meta.start_time.elapsed().as_millis() as i32;
                        let log_ctx = LogContext {
                            protocol: forward_meta.protocol,
                            duration_ms,
                            is_stream: forward_meta.is_stream,
                            info: UsageInfo::default(),
                            status: log_status::FAILED.to_string(),
                            error_message: Some(error_text.clone()),
                        };
                        spawn_usage_log(model, map, cred, &api_key_ctx, &log_ctx);
                        return Err(forward_meta.protocol.bad_request(&format!("Upstream provider error: {}", error_text)));
                    }
                }
            }
        }
    }

    let duration_ms = forward_meta.start_time.elapsed().as_millis() as i32;
    if let Some((model, map, cred)) = last_log_info {
        let log_ctx = LogContext {
            protocol: forward_meta.protocol,
            duration_ms,
            is_stream: forward_meta.is_stream,
            info: UsageInfo::default(),
            status: log_status::FAILED.to_string(),
            error_message: Some("All providers failed".to_string()),
        };
        spawn_usage_log(model, map, cred, &api_key_ctx, &log_ctx);
    }

    let error_summary = if error_details.is_empty() {
        "All models unavailable".to_string()
    } else {
        format!("All providers failed: {}", error_details.join("; "))
    };
    Err(forward_meta.protocol.service_unavailable(&error_summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_close_event_is_done_frame() {
        assert_eq!(sse_close_event().as_ref(), b"data: [DONE]\n\n");
    }
}
