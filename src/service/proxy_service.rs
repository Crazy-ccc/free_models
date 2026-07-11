use actix_web::http::StatusCode;
use actix_web::web;
use actix_web::HttpResponse;
use futures_util::{StreamExt};
use log::{error, info, warn};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::error;
use crate::service::model_service::ModelProviderInfo;
use crate::service::penalty::PriorityPenalty;

/// 上游转发结果：区分“成功”“需重试”“客户端错误直接返回”三种情况
enum ForwardOutcome {
    /// 上游返回 2xx，携带响应供调用方构造最终回复
    Success(reqwest::Response),
    /// 上游返回 4xx 5xx 或网络错误，应跳过并尝试下一个 provider
    Retry,
}

/// 上游 SSE 流中段出错时，下发一个干净的结束帧，让客户端以正常方式收尾，
/// 而不是因为传输层报错导致下游出现 “SSE stream error”。
fn sse_close_event() -> web::Bytes {
    web::Bytes::from_static(b"data: [DONE]\n\n")
}

/// 向单个 provider 发送 /chat/completions 请求并做初步状态码判断。
///
/// 公共的“构建 URL + 设置 header + 发送请求 + 4xx/5xx/网络错误分流”逻辑都收敛于此，
/// 两个公开函数（流式/非流式）仅负责把 `Success` 响应转换成对应格式。
async fn forward_to_provider(
    client: &Client,
    model_info: &ModelProviderInfo,
    body: &Value,
    is_stream: bool,
) -> ForwardOutcome {
    let url = format!(
        "{}/chat/completions",
        model_info.base_url.trim_end_matches('/')
    );

    let mut request_body = body.clone();
    request_body["model"] = Value::String(model_info.model_id.clone());
    if is_stream {
        request_body["stream"] = Value::Bool(true);
    }

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", model_info.api_key))
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "identity")
        .json(&request_body)
        .timeout(Duration::from_secs(model_info.timeout))
        .send()
        .await;

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                return ForwardOutcome::Success(resp);
            }
            let status = resp.status();
            warn!(
                "Provider {} returned error status: {}, error: {}",
                model_info.provider_name, status, serde_json::to_string(&resp.text().await.unwrap_or(String::new())).unwrap_or(String::new())
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

/// 所有 provider 均不可用时的统一 503 响应
fn all_models_unavailable(models: &[ModelProviderInfo]) -> HttpResponse {
    let model_name = models
        .first()
        .map(|m| m.model_name.as_str())
        .unwrap_or("unknown");
    let message = format!("All models unavailable for: {}", model_name);
    error::service_unavailable(&message)
}

/// 非流式代理转发（带故障切换）
pub async fn proxy_chat_completion(
    client: &Client,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
) -> Result<HttpResponse, HttpResponse> {
    for model_info in models {
        info!(
            "Trying model: {} via provider: {} (priority: {})",
            model_info.model_name, model_info.provider_name, model_info.priority
        );

        match forward_to_provider(client, model_info, body, false).await {
            ForwardOutcome::Success(resp) => {
                let status = StatusCode::from_u16(resp.status().as_u16())
                    .unwrap_or(StatusCode::OK);

                let body_bytes = resp.bytes().await.map_err(|e| {
                    error!("Failed to read response body: {}", e);
                    error::internal_error("Failed to read upstream response")
                })?;

                return Ok(HttpResponse::build(status)
                    .content_type("application/json")
                    .body(body_bytes.to_vec()));
            }
            ForwardOutcome::Retry => {
                penalty.penalize(&model_info.model_name, &model_info.provider_name);
                warn!("Model {} via provider {} failed, penalized for 30 minutes", model_info.model_name, model_info.provider_name);
                continue;
            }
        }
    }

    Err(all_models_unavailable(models))
}

/// 流式代理转发（SSE，带故障切换）
pub async fn proxy_chat_completion_stream(
    client: &Client,
    body: &Value,
    models: &[ModelProviderInfo],
    penalty: &web::Data<PriorityPenalty>,
) -> Result<HttpResponse, HttpResponse> {
    for model_info in models {
        info!(
            "Trying streaming model: {} via provider: {} (priority: {})",
            model_info.model_name, model_info.provider_name, model_info.priority
        );

        match forward_to_provider(client, model_info, body, true).await {
            ForwardOutcome::Success(resp) => {
                let byte_stream = resp.bytes_stream();

                // 将 reqwest 的流转换为 actix 的流式响应。
                // 上游中段出错时不抛 server error（响应头已发出会直接断连导致客户端 SSE stream error），
                // 而是下发一个干净的 [DONE] 结束帧，让客户端正常收尾。
                let stream = byte_stream.map(|item| match item {
                    Ok(bytes) => Ok::<_, actix_web::Error>(web::Bytes::from(bytes.to_vec())),
                    Err(_) => Ok::<_, actix_web::Error>(sse_close_event()),
                });

                return Ok(HttpResponse::Ok()
                    .content_type("text/event-stream")
                    .streaming(stream));
            }
            ForwardOutcome::Retry => {
                penalty.penalize(&model_info.model_name, &model_info.provider_name);
                warn!("Model {} via provider {} failed, penalized for 30 minutes", model_info.model_name, model_info.provider_name);
                continue;
            }
        }
    }

    Err(all_models_unavailable(models))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_close_event_is_done_frame() {
        assert_eq!(sse_close_event().as_ref(), b"data: [DONE]\n\n");
    }
}
