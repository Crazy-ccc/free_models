use actix_web::{web, HttpResponse};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use crate::response;
use crate::service::{api_key_service, model_service_ext, provider_credential_service, provider_service, proxy_service};
use crate::AppState;
use crate::db::entities::{api_key, model_config, provider_config, provider_credential};
use crate::util::crud;
use crate::util::encryption;

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

#[derive(Deserialize)]
pub struct CreateModelRequest {
    pub provider_id: i32,
    pub name: String,
    pub model_id: String,
    pub timeout: i32,
    pub protocols: String,
    pub priority: i32,
    pub status: String,
    pub context_length: i32,
}

#[derive(Deserialize)]
pub struct UpdateModelRequest {
    pub provider_id: i32,
    pub name: String,
    pub model_id: String,
    pub timeout: i32,
    pub protocols: String,
    pub priority: i32,
    pub status: String,
    pub context_length: i32,
}

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub key_value: Option<String>,
    pub name: String,
    pub is_active: bool,
}

#[derive(Deserialize)]
pub struct UpdateApiKeyRequest {
    pub key_value: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(Deserialize)]
pub struct CreateProviderCredentialRequest {
    pub provider_id: i32,
    pub name: Option<String>,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateProviderCredentialRequest {
    pub provider_id: i32,
    pub name: Option<String>,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListCredentialsQuery {
    pub provider_id: Option<i32>,
}

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

#[derive(Serialize)]
pub struct ProviderCredentialResponse {
    pub id: i32,
    pub provider_id: i32,
    pub name: String,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub created_time: chrono::NaiveDateTime,
    pub last_updated: chrono::NaiveDateTime,
}

impl ProviderCredentialResponse {
    fn from_model(m: provider_credential::Model, key: &[u8; 32]) -> Result<Self, String> {
        let api_key = encryption::decrypt(&m.api_key, key)?;
        let password = match m.encrypted_password {
            Some(p) => Some(encryption::decrypt(&p, key)?),
            None => None,
        };
        let masked_key = if api_key.len() > 4 {
            format!("{}****", &api_key[..4])
        } else {
            "****".to_string()
        };
        Ok(ProviderCredentialResponse {
            id: m.id,
            provider_id: m.provider_id,
            name: m.name,
            api_key: masked_key,
            account: m.account,
            password: password.map(|_| "****".to_string()),
            priority: m.priority,
            is_active: m.is_active,
            created_time: m.created_time,
            last_updated: m.last_updated,
        })
    }
}

fn credential_result_response(
    result: Result<provider_credential::Model, sea_orm::DbErr>,
    encryption_key: &[u8; 32],
) -> HttpResponse {
    match result {
        Ok(m) => match ProviderCredentialResponse::from_model(m, encryption_key) {
            Ok(resp) => HttpResponse::Ok().json(resp),
            Err(e) => {
                eprintln!("Decrypt error: {}", e);
                response::db_error()
            }
        },
        Err(e) => {
            eprintln!("{}", e);
            response::db_error()
        }
    }
}

pub async fn get_service_status(state: web::Data<AppState>) -> HttpResponse {
    let active_models = model_service_ext::count_active(&state.db).await;
    let total_models = crud::count_total::<model_config::Entity>(&state.db).await;
    let active_credentials = provider_credential::Entity::find()
        .filter(provider_credential::Column::IsActive.eq(true))
        .all(&state.db)
        .await;
    let total_providers = crud::count_total::<provider_config::Entity>(&state.db).await;
    let active_api_keys =
        crud::count_active::<api_key::Entity, _>(&state.db, api_key::Column::IsActive).await;
    let total_api_keys = crud::count_total::<api_key::Entity>(&state.db).await;

    let active_providers = match &active_credentials {
        Ok(list) => {
            let unique_providers: std::collections::HashSet<i32> = list.iter().map(|c| c.provider_id).collect();
            Ok(unique_providers.len() as u64)
        }
        Err(e) => Err(sea_orm::DbErr::Custom(e.to_string())),
    };

    let counts = [&active_models, &total_models, &active_providers, &total_providers, &active_api_keys, &total_api_keys];
    for res in &counts {
        if res.is_err() {
            return response::db_error();
        }
    }

    let am = active_models.unwrap_or(0);
    let tm = total_models.unwrap_or(0);
    let ap = active_providers.unwrap_or(0);
    let tp = total_providers.unwrap_or(0);
    let ak = active_api_keys.unwrap_or(0);
    let tk = total_api_keys.unwrap_or(0);

    HttpResponse::Ok().json(serde_json::json!({
        "healthy": true,
        "models": {
            "total": tm,
            "active": am,
            "inactive": tm - am,
        },
        "providers": {
            "total": tp,
            "active": ap,
            "inactive": tp - ap,
        },
        "apiKeys": {
            "total": tk,
            "active": ak,
            "inactive": tk - ak,
        },
        "penalties": []
    }))
}

pub async fn refresh_cache(state: web::Data<AppState>) -> HttpResponse {
    state.model_cache.clear().await;
    state.provider_cache.clear().await;
    state.api_key_cache.refresh(&state.db).await;
    let _ = state.redis.del_pattern("penalty:*").await;
    log::info!("Cache refreshed manually");
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

macro_rules! handle_result {
    ($expr:expr) => {
        match $expr {
            Ok(v) => HttpResponse::Ok().json(v),
            Err(e) => {
                eprintln!("{}", e);
                response::db_error()
            }
        }
    };
}

macro_rules! handle_find {
    ($expr:expr, $not_found:expr) => {
        match $expr {
            Ok(Some(v)) => HttpResponse::Ok().json(v),
            Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": $not_found})),
            Err(e) => {
                eprintln!("{}", e);
                response::db_error()
            }
        }
    };
}

macro_rules! handle_delete {
    ($expr:expr, $not_found:expr) => {
        match $expr {
            Ok(true) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
            Ok(false) => HttpResponse::NotFound().json(serde_json::json!({"error": $not_found})),
            Err(e) => {
                eprintln!("{}", e);
                response::db_error()
            }
        }
    };
}

// ─── Provider ──────────────────────────────────────────────────────────

pub async fn list_providers(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(provider_service::list(&state.db).await)
}

pub async fn get_provider(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(provider_service::get(&state.db, path.into_inner()).await, "Provider not found")
}

pub async fn create_provider(
    state: web::Data<AppState>,
    body: web::Json<CreateProviderRequest>,
) -> HttpResponse {
    if !is_valid_base_url(&body.base_url) {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid base_url format"}));
    }
    handle_result!(provider_service::create(&state.db, &body.name, &body.base_url).await)
}

pub async fn update_provider(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateProviderRequest>,
) -> HttpResponse {
    if !is_valid_base_url(&body.base_url) {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid base_url format"}));
    }
    handle_result!(provider_service::update(&state.db, path.into_inner(), &body.name, &body.base_url).await)
}

fn is_valid_base_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub async fn delete_provider(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(provider_service::delete(&state.db, path.into_inner()).await, "Provider not found")
}

// ─── Model ─────────────────────────────────────────────────────────────

pub async fn list_models(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(model_service_ext::list(&state.db).await)
}

pub async fn get_model(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(model_service_ext::get(&state.db, path.into_inner()).await, "Model not found")
}

pub async fn create_model(
    state: web::Data<AppState>,
    body: web::Json<CreateModelRequest>,
) -> HttpResponse {
    handle_result!(model_service_ext::create(
        &state.db, body.provider_id, &body.name, &body.model_id,
        body.timeout, &body.protocols, body.priority, body.status.clone(),
        body.context_length,
    ).await)
}

pub async fn update_model(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateModelRequest>,
) -> HttpResponse {
    handle_result!(model_service_ext::update(
        &state.db, path.into_inner(), body.provider_id, &body.name, &body.model_id,
        body.timeout, &body.protocols, body.priority, body.status.clone(),
        body.context_length,
    ).await)
}

pub async fn delete_model(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(model_service_ext::delete(&state.db, path.into_inner()).await, "Model not found")
}

// ─── API Key ───────────────────────────────────────────────────────────

pub async fn list_api_keys(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(api_key_service::list(&state.db).await)
}

pub async fn get_api_key(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(api_key_service::get(&state.db, path.into_inner()).await, "API key not found")
}

pub async fn create_api_key(
    state: web::Data<AppState>,
    body: web::Json<CreateApiKeyRequest>,
) -> HttpResponse {
    handle_result!(api_key_service::create(&state.db, body.key_value.clone(), &body.name, body.is_active).await)
}

pub async fn update_api_key(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateApiKeyRequest>,
) -> HttpResponse {
    handle_result!(api_key_service::update(&state.db, path.into_inner(), &body.key_value, &body.name, body.is_active).await)
}

pub async fn delete_api_key(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(api_key_service::delete(&state.db, path.into_inner()).await, "API key not found")
}

// ─── Provider Credential ──────────────────────────────────────────────

pub async fn list_provider_credentials(
    state: web::Data<AppState>,
    query: web::Query<ListCredentialsQuery>,
) -> HttpResponse {
    let credentials = match query.provider_id {
        Some(pid) => provider_credential_service::list_by_provider(&state.db, pid).await,
        None => provider_credential::Entity::find().all(&state.db).await,
    };
    match credentials {
        Ok(list) => {
            let results: Vec<ProviderCredentialResponse> = list
                .into_iter()
                .filter_map(|m| ProviderCredentialResponse::from_model(m, &state.encryption_key).ok())
                .collect();
            HttpResponse::Ok().json(results)
        }
        Err(e) => {
            eprintln!("{}", e);
            response::db_error()
        }
    }
}

pub async fn get_provider_credential(
    state: web::Data<AppState>,
    path: web::Path<i32>,
) -> HttpResponse {
    match provider_credential_service::get(&state.db, path.into_inner()).await {
        Ok(Some(m)) => credential_result_response(Ok(m), &state.encryption_key),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "Credential not found"})),
        Err(e) => credential_result_response(Err(e), &state.encryption_key),
    }
}

pub async fn create_provider_credential(
    state: web::Data<AppState>,
    body: web::Json<CreateProviderCredentialRequest>,
) -> HttpResponse {
    let result = provider_credential_service::create(
        &state.db,
        body.provider_id,
        body.name.as_deref().unwrap_or(""),
        &body.api_key,
        body.account.as_deref(),
        body.password.as_deref(),
        body.priority.unwrap_or(0),
        body.is_active.unwrap_or(true),
        &state.encryption_key,
    ).await;
    credential_result_response(result, &state.encryption_key)
}

pub async fn update_provider_credential(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateProviderCredentialRequest>,
) -> HttpResponse {
    let result = provider_credential_service::update(
        &state.db,
        path.into_inner(),
        body.provider_id,
        body.name.as_deref().unwrap_or(""),
        &body.api_key,
        body.account.as_deref(),
        body.password.as_deref(),
        body.priority.unwrap_or(0),
        body.is_active.unwrap_or(true),
        &state.encryption_key,
    ).await;
    credential_result_response(result, &state.encryption_key)
}

pub async fn delete_provider_credential(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(provider_credential_service::delete(&state.db, path.into_inner()).await, "Credential not found")
}

pub async fn test_credential(
    state: web::Data<AppState>,
    body: web::Json<TestCredentialRequest>,
) -> HttpResponse {
    use std::time::Instant;

    // 1. Get credential
    let cred = match provider_credential::Entity::find_by_id(body.credential_id)
        .one(&state.db)
        .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Credential not found"})),
        Err(e) => {
            eprintln!("{}", e);
            return response::db_error();
        }
    };

    // 2. Decrypt api_key
    let api_key = match encryption::decrypt(&cred.api_key, &state.encryption_key) {
        Ok(k) => k,
        Err(e) => return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("Decrypt failed: {}", e)})),
    };

    // 3. Get provider
    let provider = match provider_config::Entity::find_by_id(cred.provider_id)
        .one(&state.db)
        .await
    {
        Ok(Some(p)) => p,
        _ => return HttpResponse::NotFound().json(serde_json::json!({"error": "Provider not found"})),
    };

    // 4. Get model config by model_id
    let model = match model_config::Entity::find()
        .filter(model_config::Column::ModelId.eq(&body.model_id))
        .filter(model_config::Column::ProviderId.eq(cred.provider_id))
        .one(&state.db)
        .await
    {
        Ok(Some(m)) => m,
        Ok(None) => return HttpResponse::NotFound().json(serde_json::json!({"error": "Model not found for this provider"})),
        Err(e) => {
            eprintln!("{}", e);
            return response::db_error();
        }
    };

    // 5. Determine protocol
    let use_anthropic = model.protocols.split(',')
        .map(|s| s.trim())
        .any(|s| s == "anthropic");

    let url = if use_anthropic {
        format!("{}/messages", provider.base_url.trim_end_matches('/'))
    } else {
        format!("{}/chat/completions", provider.base_url.trim_end_matches('/'))
    };

    if let Err(e) = proxy_service::validate_url_safe(&url).await {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": format!("SSRF check failed: {}", e)}));
    }

    let prompt_text = body.prompt.clone().unwrap_or_else(|| "Hello".to_string());
    let request_body = serde_json::json!({
        "model": model.model_id,
        "max_tokens": 10,
        "messages": [{"role": "user", "content": prompt_text}]
    });

    let start = Instant::now();
    let mut request = state.client
        .post(&url)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30));

    if use_anthropic {
        request = request
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = request.json(&request_body).send().await;
    let elapsed_ms = start.elapsed().as_millis() as i32;

    match response {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                HttpResponse::Ok().json(TestCredentialResponse {
                    success: true,
                    response_time_ms: elapsed_ms,
                    model_id: body.model_id.clone(),
                    provider_id: cred.provider_id,
                    credential_id: cred.id,
                    error: None,
                })
            } else {
                let error_text = resp.text().await.unwrap_or_else(|_| status.to_string());
                HttpResponse::Ok().json(TestCredentialResponse {
                    success: false,
                    response_time_ms: elapsed_ms,
                    model_id: body.model_id.clone(),
                    provider_id: cred.provider_id,
                    credential_id: cred.id,
                    error: Some(format!("HTTP {}: {}", status, error_text)),
                })
            }
        }
        Err(e) => {
            HttpResponse::Ok().json(TestCredentialResponse {
                success: false,
                response_time_ms: elapsed_ms,
                model_id: body.model_id.clone(),
                provider_id: cred.provider_id,
                credential_id: cred.id,
                error: Some(e.to_string()),
            })
        }
    }
}

// ─── Routes ────────────────────────────────────────────────────────────

pub fn admin_routes() -> actix_web::Scope {
    web::scope("/admin")
        .route("/service/status", web::get().to(get_service_status))
        .route("/cache/refresh", web::post().to(refresh_cache))
        .route("/providers", web::get().to(list_providers))
        .route("/providers", web::post().to(create_provider))
        .route("/providers/{id}", web::get().to(get_provider))
        .route("/providers/{id}", web::put().to(update_provider))
        .route("/providers/{id}", web::delete().to(delete_provider))
        .route("/models", web::get().to(list_models))
        .route("/models", web::post().to(create_model))
        .route("/models/{id}", web::get().to(get_model))
        .route("/models/{id}", web::put().to(update_model))
        .route("/models/{id}", web::delete().to(delete_model))
        .route("/api_keys", web::get().to(list_api_keys))
        .route("/api_keys", web::post().to(create_api_key))
        .route("/api_keys/{id}", web::get().to(get_api_key))
        .route("/api_keys/{id}", web::put().to(update_api_key))
        .route("/api_keys/{id}", web::delete().to(delete_api_key))
        .route("/provider_credentials", web::get().to(list_provider_credentials))
        .route("/provider_credentials", web::post().to(create_provider_credential))
        .route("/provider_credentials/{id}", web::get().to(get_provider_credential))
        .route("/provider_credentials/{id}", web::put().to(update_provider_credential))
        .route("/provider_credentials/{id}", web::delete().to(delete_provider_credential))
        .route("/test_credential", web::post().to(test_credential))
}
