use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::response;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub key_value: Option<String>,
    pub name: String,
    pub is_active: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn list_api_keys(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(state.database.api_keys.list().await)
}

pub async fn get_api_key(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(state.database.api_keys.get(path.into_inner()).await, "API key not found")
}

/// 操作成功时刷新 API key 缓存
async fn refresh_api_key_cache(state: &AppState, ok: bool) {
    if ok {
        state.api_key_cache.refresh(&state.database.api_keys).await;
    }
}

pub async fn create_api_key(
    state: web::Data<AppState>,
    body: web::Json<CreateApiKeyRequest>,
) -> HttpResponse {
    let result = state.database.api_keys.create(body.key_value.clone(), &body.name, body.is_active).await;
    refresh_api_key_cache(&state, result.is_ok()).await;
    handle_result!(result)
}

pub async fn update_api_key(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateApiKeyRequest>,
) -> HttpResponse {
    let result = state.database.api_keys.update(
        path.into_inner(),
        None,
        body.name.as_deref(),
        body.is_active,
    ).await;
    refresh_api_key_cache(&state, result.is_ok()).await;
    handle_result!(result)
}

pub async fn delete_api_key(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    let result = state.database.api_keys.delete(path.into_inner()).await;
    refresh_api_key_cache(&state, result.is_ok()).await;
    handle_delete!(result, "API key not found")
}
