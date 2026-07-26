use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::response;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateModelRequest {
    pub name: String,
    pub timeout: i32,
    pub priority: i32,
    pub is_active: Option<bool>,
    pub context_length: i32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct UpdateModelRequest {
    pub name: Option<String>,
    pub timeout: Option<i32>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
    pub context_length: Option<i32>,
}

pub async fn list_models(state: web::Data<AppState>) -> HttpResponse {
    handle_result!(state.database.model_configs.list().await)
}

pub async fn get_model(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(state.database.model_configs.get(path.into_inner()).await, "Model not found")
}

pub async fn create_model(
    state: web::Data<AppState>,
    body: web::Json<CreateModelRequest>,
) -> HttpResponse {
    handle_result!(state.database.model_configs.create(
        &body.name, body.timeout,
        body.priority, body.context_length,
        body.is_active.unwrap_or(true),
    ).await)
}

pub async fn update_model(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateModelRequest>,
) -> HttpResponse {
    handle_result!(state.database.model_configs.update(
        path.into_inner(),
        body.name.as_deref(),
        body.timeout,
        body.priority,
        body.context_length,
        body.is_active,
    ).await)
}

pub async fn delete_model(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(state.database.model_configs.delete(path.into_inner()).await, "Model not found")
}
