use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::web;
use actix_web::HttpResponseBuilder;
use log::{debug, error, info, warn};
use reqwest::Client;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::time::Duration;

use crate::db::impls::ProviderCredentialStoreSeaorm;
use crate::util::cache_affinity::CacheAffinity;
use crate::util::model_scheduler::{CredentialInfo, ModelProviderMap, ModelScheduleInfo};
pub(crate) use crate::util::penalty::CircuitBreaker;
pub(crate) use crate::util::proxy_ssrf::{SsrfChecker, validate_url_safe};
pub(crate) use crate::util::proxy_types::{ApiKeyContext, ForwardMeta, LogContext, Protocol, UsageInfo, log_status};
use crate::util::stream_usage_scanner::{SseScanner, extract_error_message, is_quota_error, scan_error_json, scan_usage_json};
use crate::util::usage_log_collector::spawn_usage_log;

pub(crate) fn join_url(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name,
        "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "upgrade"
            | "x-accel-buffering"
            | "x-accel-limit-rate"
            | "x-accel-redirect"
            | "x-forwarded-for"
            | "x-forwarded-proto"
    )
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

enum ForwardOutcome {
    Success(reqwest::Response),
    Retry(Option<u64>),
    Fail(String),
    CredentialFail(String),
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
    circuit_breaker: &web::Data<CircuitBreaker>,
    cred_store: &ProviderCredentialStoreSeaorm,
    cache_affinity: &CacheAffinity,
    api_key_ctx: &ApiKeyContext,
    forward_meta: ForwardMeta,
) -> Result<HttpResponse, HttpResponse> {
    if forward_meta.is_stream {
        Ok(handle_stream_response(
            resp,
            model_info,
            map,
            cred,
            circuit_breaker,
            cred_store.clone(),
            cache_affinity,
            api_key_ctx,
            forward_meta.protocol,
            forward_meta.start_time,
        )
        .await)
    } else {
        handle_non_stream_response(
            resp,
            model_info,
            map,
            cred,
            circuit_breaker,
            cred_store,
            cache_affinity,
            api_key_ctx,
            forward_meta.protocol,
            forward_meta.start_time,
        )
        .await
    }
}

fn parse_retry_after(header: Option<&str>) -> Option<u64> {
    let trimmed = header?.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
        return trimmed.parse::<u64>().ok();
    }
    chrono::DateTime::parse_from_rfc2822(trimmed)
        .ok()
        .map(|dt| (dt.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_seconds().max(0) as u64)
}

fn retry_delay(retry_after: Option<u64>) -> Duration {
    match retry_after {
        Some(secs) => Duration::from_secs(secs.min(15)),
        None => Duration::from_millis(500 + rand::random_range(0..=400)),
    }
}

fn find_embedded_error(body: &[u8]) -> Option<String> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let error = json.get("error").filter(|e| !e.is_null())?;
    Some(extract_error_message(error))
}

async fn forward_to_provider(
    client: &Client,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    body: &Value,
    is_stream: bool,
    protocol: Protocol,
    ssrf_checker: &SsrfChecker,
) -> ForwardOutcome {
    let url = join_url(&map.base_url, protocol.path());

    if let Err(e) = ssrf_checker.validate_url_safe(&url).await {
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
                let retry_after = parse_retry_after(resp.headers().get("retry-after").and_then(|v| v.to_str().ok()));
                let error_text = resp.text().await.unwrap_or_default();
                warn!(
                    "Provider {} returned error status: {}, error: {}",
                    map.provider_name, status, error_text
                );
                match status.as_u16() {
                    401 | 402 | 403 => ForwardOutcome::CredentialFail(error_text),
                    429 => {
                        if is_quota_error(&error_text) {
                            ForwardOutcome::CredentialFail(error_text)
                        } else {
                            ForwardOutcome::Retry(retry_after)
                        }
                    }
                    500 | 502 | 503 | 504 => ForwardOutcome::Retry(retry_after),
                    _ => ForwardOutcome::Fail(error_text),
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
            ForwardOutcome::Retry(None)

        }
    }
}

fn append_upstream_headers(builder: &mut HttpResponseBuilder, upstream: &HeaderMap) -> bool {
    let mut has_content_type = false;
    for (name, value) in upstream.iter() {
        if is_hop_by_hop_header(name.as_str()) {
            continue;
        }
        let Ok(header_str) = value.to_str() else {
            continue;
        };
        if name.as_str().eq_ignore_ascii_case("content-type") {
            builder.content_type(header_str);
            has_content_type = true;
        } else if let Ok(header_value) = actix_web::http::header::HeaderValue::from_str(header_str) {
            builder.append_header((name.as_str(), header_value));
        }
    }
    has_content_type
}

async fn handle_embedded_error(
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    circuit_breaker: &web::Data<CircuitBreaker>,
    cred_store: &ProviderCredentialStoreSeaorm,
    api_key_ctx: &ApiKeyContext,
    protocol: Protocol,
    start_time: std::time::Instant,
    err_msg: &str,
    is_quota: bool,
    is_stream: bool,
) {
    warn!(
        "Model {} via provider {} credential {} embedded error: {}",
        model_info.model_name, map.provider_name, cred.provider_credential_id, err_msg
    );
    circuit_breaker.record_failure(&model_info.model_name, &map.provider_name, cred.provider_credential_id);
    if is_quota {
        if let Err(e) = cred_store.mark_quota_exhausted(cred.provider_credential_id).await {
            log::error!("Failed to mark credential {} quota exhausted: {}", cred.provider_credential_id, e);
        }
    }
    let log_ctx = LogContext {
        protocol,
        duration_ms: start_time.elapsed().as_millis() as i32,
        is_stream,
        info: UsageInfo::default(),
        status: log_status::FAILED.to_string(),
        error_message: Some(err_msg.to_string()),
    };
    spawn_usage_log(model_info, map, cred, api_key_ctx, &log_ctx);
}

async fn handle_stream_response(
    resp: reqwest::Response,
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    circuit_breaker: &web::Data<CircuitBreaker>,
    cred_store: ProviderCredentialStoreSeaorm,
    cache_affinity: &CacheAffinity,
    api_key_ctx: &ApiKeyContext,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> HttpResponse {
    use futures_util::StreamExt;

    record_success_metrics(model_info, map, cred, circuit_breaker, cache_affinity, api_key_ctx.id).await;

    let mut client_resp_builder = HttpResponse::Ok();
    client_resp_builder.content_type("text/event-stream");
    append_upstream_headers(&mut client_resp_builder, resp.headers());

    let model_info = model_info.clone();
    let map = map.clone();
    let cred = cred.clone();
    let api_key_ctx = api_key_ctx.clone();
    let circuit_breaker = circuit_breaker.clone();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<web::Bytes, actix_web::Error>>(64);

    actix_web::rt::spawn(async move {
        let mut upstream_stream = resp.bytes_stream();
        let mut scanner = SseScanner::new();
        let mut error_detector = SseScanner::new();

        while let Some(item) = upstream_stream.next().await {
            match item {
                Ok(bytes) => {
                    if !scanner.done {
                        if let Some(info) = scanner.push(&bytes, |json| scan_usage_json(json, protocol)) {
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
                        }
                    }

                    if !error_detector.done {
                        if let Some((err_msg, is_quota)) = error_detector.push(&bytes, scan_error_json) {
                            handle_embedded_error(
                                &model_info, &map, &cred, &circuit_breaker, &cred_store,
                                &api_key_ctx, protocol, start_time, &err_msg, is_quota, true,
                            )
                            .await;
                            break;
                        }
                    }

                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        if !scanner.done {
            if let Some(info) = scanner.finish(|json| scan_usage_json(json, protocol)) {
                log_stream_usage(
                    &model_info, &map, &cred,
                    &api_key_ctx,
                    protocol,
                    start_time,
                    info,
                );
            }
        }

        if !error_detector.done {
            if let Some((err_msg, is_quota)) = error_detector.finish(scan_error_json) {
                handle_embedded_error(
                    &model_info, &map, &cred, &circuit_breaker, &cred_store,
                    &api_key_ctx, protocol, start_time, &err_msg, is_quota, true,
                )
                .await;
            }
        }
    });

    let rx_stream = futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(item) => Some((item, rx)),
            None => None,
        }
    });

    client_resp_builder.streaming(rx_stream)
}

async fn handle_non_stream_response(
    resp: reqwest::Response,
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    circuit_breaker: &web::Data<CircuitBreaker>,
    cred_store: &ProviderCredentialStoreSeaorm,
    cache_affinity: &CacheAffinity,
    api_key_ctx: &ApiKeyContext,
    protocol: Protocol,
    start_time: std::time::Instant,
) -> Result<HttpResponse, HttpResponse> {
    let status = StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(StatusCode::OK);

    let mut client_resp = HttpResponse::build(status);
    let has_content_type = append_upstream_headers(&mut client_resp, resp.headers());

    let body_bytes = resp.bytes().await.map_err(|e| {
        error!("Failed to read response body: {}", e);
        protocol.internal_error("Failed to read upstream response")
    })?;

    if let Some(err_msg) = find_embedded_error(&body_bytes) {
        handle_embedded_error(
            model_info, map, cred, circuit_breaker, cred_store, api_key_ctx,
            protocol, start_time, &err_msg, is_quota_error(&err_msg), false,
        )
        .await;
        return Ok(client_resp.body(body_bytes.to_vec()));
    }

    record_success_metrics(model_info, map, cred, circuit_breaker, cache_affinity, api_key_ctx.id).await;

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

    if !has_content_type {
        client_resp.content_type("application/json");
    }
    Ok(client_resp.body(body_bytes.to_vec()))
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
            let (provider_matched, rest): (Vec<ModelProviderMap>, Vec<ModelProviderMap>) =
                std::mem::take(&mut model.maps)
                    .into_iter()
                    .partition(|map| map.provider_name == entry.provider_name);
            let (mut affinity_maps, same_provider): (Vec<ModelProviderMap>, Vec<ModelProviderMap>) =
                provider_matched
                    .into_iter()
                    .partition(|map| {
                        map.credentials
                            .iter()
                            .any(|c| c.provider_credential_id == entry.provider_credential_id)
                    });
            // 保留最后一个匹配（与原实现 affinity_map 覆盖语义一致）
            let affinity_map = affinity_maps.pop();
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

fn record_credential_fail(
    circuit_breaker: &web::Data<CircuitBreaker>,
    cred_store: &ProviderCredentialStoreSeaorm,
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    error_text: &str,
    after_retry: bool,
    error_details: &mut Vec<String>,
) {
    circuit_breaker.record_failure(&model_info.model_name, &map.provider_name, cred.provider_credential_id);
    let store = cred_store.clone();
    let cred_id = cred.provider_credential_id;
    tokio::spawn(async move {
        if let Err(e) = store.mark_quota_exhausted(cred_id).await {
            log::error!("Failed to mark credential {} quota exhausted: {}", cred_id, e);
        }
    });
    if after_retry {
        warn!("Model {} via provider {} credential {} returned credential error after retry: {}", model_info.model_name, map.provider_name, cred.provider_credential_id, error_text);
    } else {
        warn!("Model {} via provider {} credential {} returned credential error: {}", model_info.model_name, map.provider_name, cred.provider_credential_id, error_text);
    }
    error_details.push(format!("{} via {} credential {}: {}", model_info.model_name, map.provider_name, cred.provider_credential_id, error_text));
}

pub(crate) async fn proxy_chat_completion_inner(
    client: &Client,
    body: &Value,
    models: &[ModelScheduleInfo],
    circuit_breaker: &web::Data<CircuitBreaker>,
    cache_affinity: &CacheAffinity,
    api_key_ctx: ApiKeyContext,
    forward_meta: ForwardMeta,
    cred_store: &ProviderCredentialStoreSeaorm,
    ssrf_checker: &SsrfChecker,
) -> Result<HttpResponse, HttpResponse> {
    let ordered_models = reorder_by_affinity(models, api_key_ctx.id, cache_affinity);
    let mut last_log_info: Option<(&ModelScheduleInfo, &ModelProviderMap, &CredentialInfo)> = None;
    let mut error_details: Vec<String> = Vec::new();

    for model in &ordered_models {
        for map in &model.maps {
            for cred in &map.credentials {
                if cred.quota_exhausted {
                    info!(
                        "Skipping model {} via provider {} credential {} (quota exhausted)",
                        model.model_name, map.provider_name, cred.provider_credential_id
                    );
                    error_details.push(format!("{} via {} credential {}: quota exhausted", model.model_name, map.provider_name, cred.provider_credential_id));
                    continue;
                }
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

                match forward_to_provider(client, map, cred, body, forward_meta.is_stream, forward_meta.protocol, ssrf_checker).await {
                    ForwardOutcome::Success(resp) => {
                        return handle_success(resp, model, map, cred, circuit_breaker, cred_store, cache_affinity, &api_key_ctx, forward_meta).await;
                    }
                    ForwardOutcome::Retry(retry_after) => {
                        let wait = retry_delay(retry_after);
                        warn!("Model {} via provider {} credential {} returned retryable error, retrying after {:?}...", model.model_name, map.provider_name, cred.provider_credential_id, wait);
                        tokio::time::sleep(wait).await;
                        match forward_to_provider(client, map, cred, body, forward_meta.is_stream, forward_meta.protocol, ssrf_checker).await {
                            ForwardOutcome::Success(resp) => {
                                return handle_success(resp, model, map, cred, circuit_breaker, cred_store, cache_affinity, &api_key_ctx, forward_meta).await;
                            }
                            ForwardOutcome::Retry(_) | ForwardOutcome::Fail(_) => {
                                circuit_breaker.record_failure(&model.model_name, &map.provider_name, cred.provider_credential_id);
                                warn!("Model {} via provider {} credential {} failed after retry, circuit breaker updated", model.model_name, map.provider_name, cred.provider_credential_id);
                                error_details.push(format!("{} via {} credential {}: failed after retry", model.model_name, map.provider_name, cred.provider_credential_id));
                                continue;
                            }
                            ForwardOutcome::CredentialFail(error_text) => {
                                record_credential_fail(circuit_breaker, cred_store, model, map, cred, &error_text, true, &mut error_details);
                                continue;
                            }
                        }
                    }
                    ForwardOutcome::CredentialFail(error_text) => {
                        record_credential_fail(circuit_breaker, cred_store, model, map, cred, &error_text, false, &mut error_details);
                        continue;
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
