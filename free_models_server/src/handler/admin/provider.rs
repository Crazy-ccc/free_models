use actix_web::{web, HttpResponse};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::db::entities::provider_config::Model as ProviderConfig;
use crate::response;
use crate::service::proxy_service;
use crate::util::encryption;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub base_url: String,
}

#[derive(Deserialize)]
pub struct UpdateProviderRequest {
    pub name: String,
    pub base_url: String,
}

pub async fn list_providers(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(state.database.provider_configs.list().await)
}

pub async fn get_provider(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(state.database.provider_configs.get(path.into_inner()).await, "Provider not found")
}

pub async fn create_provider(
    state: web::Data<AppState>,
    body: web::Json<CreateProviderRequest>,
) -> HttpResponse {
    if proxy_service::validate_url_safe(&body.base_url).await.is_err() {
        return response::bad_request("Invalid base_url format");
    }
    handle_result!(state.database.provider_configs.create(&body.name, &body.base_url).await)
}

pub async fn update_provider(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateProviderRequest>,
) -> HttpResponse {
    if proxy_service::validate_url_safe(&body.base_url).await.is_err() {
        return response::bad_request("Invalid base_url format");
    }
    handle_result!(state.database.provider_configs.update(path.into_inner(), &body.name, &body.base_url).await)
}

pub async fn delete_provider(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(state.database.provider_configs.delete(path.into_inner()).await, "Provider not found")
}

/// Fetch the raw JSON body from a provider's `/models` endpoint after SSRF validation.
async fn fetch_provider_models(
    client: &Client,
    provider: &ProviderConfig,
    api_key: &str,
) -> Result<Value, HttpResponse> {
    let url = proxy_service::join_url(&provider.base_url, "/models");

    if let Err(e) = proxy_service::validate_url_safe(&url).await {
        log::warn!("SSRF check failed for {}: {}", url, e);
        return Err(response::bad_request("URL validation failed"));
    }

    let resp = match client.get(&url).header("Authorization", format!("Bearer {}", api_key)).timeout(std::time::Duration::from_secs(30)).send().await {
        Ok(r) => r,
        Err(e) => return Err(response::bad_gateway(&format!("Failed to fetch models: {}", e))),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| status.to_string());
        return Err(response::bad_gateway(&format!("HTTP {}: {}", status, text)));
    }

    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Err(response::bad_gateway(&format!("Invalid JSON response: {}", e))),
    };

    Ok(body)
}

/// Multi-key lookup that tries `body` (top-level array), `data`, then `models`,
/// and extracts the `id` field of each element as a String.
fn extract_model_ids(body: &Value) -> Option<Vec<String>> {
    let arr = std::iter::once(body.as_array())
        .chain(["data", "models"].iter().map(|k| body.get(k).and_then(|v| v.as_array())))
        .find_map(|opt| opt.cloned())?;
    Some(
        arr.into_iter()
            .filter_map(|item| item.get("id")?.as_str().map(|s| s.to_string()))
            .collect(),
    )
}

pub async fn get_provider_models(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    let provider_id = path.into_inner();
    let provider = match state.database.provider_configs.get(provider_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return response::not_found("Provider not found"),
        Err(_) => return response::db_error(),
    };
    let credentials = match state.database.provider_credentials.list_by_provider(provider_id).await {
        Ok(list) => list,
        Err(_) => return response::db_error(),
    };
    let api_key = match credentials.iter().find(|c| c.is_active) {
        Some(c) => match encryption::decrypt(&c.api_key, &state.encryption_key) {
            Ok(k) => k,
            Err(_) => return response::internal("Failed to decrypt credential"),
        },
        None => return response::bad_request("No active credential found for this provider"),
    };

    let body = match fetch_provider_models(&state.client, &provider, &api_key).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };
    let result: Vec<Value> = match extract_model_ids(&body) {
        Some(ids) => ids.into_iter().map(|id| serde_json::json!({ "id": id })).collect(),
        None => return response::bad_gateway("Unexpected response format"),
    };

    HttpResponse::Ok().json(result)
}
