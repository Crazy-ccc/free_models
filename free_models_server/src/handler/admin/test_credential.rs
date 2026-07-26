use actix_web::{web, HttpResponse};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::entities::provider_config::Model as ProviderConfig;
use crate::db::entities::provider_credential::Model as ProviderCredential;
use crate::db::entities::provider_model_map::Model as ProviderModelMap;
use crate::response;
use crate::service::proxy_service;
use crate::util::encryption;
use crate::AppState;

#[derive(Deserialize)]
pub struct TestCredentialRequest {
    pub credential_id: i32,
    pub model_id: String,
    pub prompt: Option<String>,
}

#[derive(Serialize)]
pub struct TestCredentialResponse {
    pub success: bool,
    pub response_time_ms: i32,
    pub model_id: String,
    pub provider_id: i32,
    pub credential_id: i32,
    pub error: Option<String>,
}

/// Resolve the credential, provider, and provider_model_map needed to run a test call.
/// Decryption of `api_key` is intentionally left to the caller.
async fn resolve_test_target(
    state: &web::Data<AppState>,
    credential_id: i32,
    model_id: &str,
) -> Result<(ProviderCredential, ProviderConfig, ProviderModelMap), HttpResponse> {
    let cred = match state.database.provider_credentials.get(credential_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return Err(response::not_found("Credential not found")),
        Err(e) => {
            log::error!("{}", e);
            return Err(response::db_error());
        }
    };
    let provider = match state.database.provider_configs.get(cred.provider_id).await {
        Ok(Some(p)) => p,
        _ => return Err(response::not_found("Provider not found")),
    };
    let maps = match state.database.provider_model_maps.list_filtered(None, Some(cred.provider_id)).await {
        Ok(m) => m,
        Err(e) => {
            log::error!("{}", e);
            return Err(response::db_error());
        }
    };
    let map = match maps.into_iter().find(|m| m.provider_model_id == model_id) {
        Some(m) => m,
        None => return Err(response::not_found("Model not found for this provider")),
    };
    Ok((cred, provider, map))
}

/// Build the JSON request body for a test chat completion.
fn build_test_request_body(map: &ProviderModelMap, prompt: &str) -> Value {
    serde_json::json!({
        "model": map.provider_model_id,
        "max_tokens": 10,
        "messages": [{"role": "user", "content": prompt}]
    })
}

/// Send the test POST request with provider-specific auth headers.
/// Anthropic uses `x-api-key` + `anthropic-version`; OpenAI uses `Authorization: Bearer`.
async fn send_test_request(
    client: &Client,
    url: &str,
    use_anthropic: bool,
    api_key: &str,
    body: Value,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30));

    if use_anthropic {
        request = request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.header("Authorization", format!("Bearer {}", api_key));
    }

    request.json(&body).send().await
}

/// Construct the `TestCredentialResponse` JSON. Lifted from the original closure.
fn build_test_response(
    success: bool,
    error: Option<String>,
    elapsed_ms: i32,
    model_id: &str,
    cred: &ProviderCredential,
) -> HttpResponse {
    HttpResponse::Ok().json(TestCredentialResponse {
        success,
        response_time_ms: elapsed_ms,
        model_id: model_id.to_string(),
        provider_id: cred.provider_id,
        credential_id: cred.id,
        error,
    })
}

pub async fn test_credential(
    state: web::Data<AppState>,
    body: web::Json<TestCredentialRequest>,
) -> HttpResponse {
    let (cred, provider, map) = match resolve_test_target(&state, body.credential_id, &body.model_id).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let api_key = match encryption::decrypt(&cred.api_key, &state.encryption_key) {
        Ok(k) => k,
        Err(e) => return response::internal(&format!("Decrypt failed: {}", e)),
    };
    let use_anthropic = map.protocols.split(',').map(|s| s.trim()).any(|s| s == "anthropic");
    let url = if use_anthropic {
        proxy_service::join_url(&provider.base_url, "/messages")
    } else {
        proxy_service::join_url(&provider.base_url, "/chat/completions")
    };
    if let Err(e) = proxy_service::validate_url_safe(&url).await {
        log::warn!("SSRF check failed for {}: {}", url, e);
        return response::bad_request("URL validation failed");
    }
    let request_body = build_test_request_body(&map, body.prompt.as_deref().unwrap_or("Hello"));
    let start = std::time::Instant::now();
    let response = send_test_request(&state.client, &url, use_anthropic, &api_key, request_body).await;
    let elapsed_ms = start.elapsed().as_millis() as i32;
    match response {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                build_test_response(true, None, elapsed_ms, &body.model_id, &cred)
            } else {
                let error_text = resp.text().await.unwrap_or_else(|_| status.to_string());
                build_test_response(false, Some(format!("HTTP {}: {}", status, error_text)), elapsed_ms, &body.model_id, &cred)
            }
        }
        Err(e) => build_test_response(false, Some(e.to_string()), elapsed_ms, &body.model_id, &cred),
    }
}
