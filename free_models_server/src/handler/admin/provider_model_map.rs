use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateProviderModelMapRequest {
    pub model_id: i32,
    pub provider_id: i32,
    pub provider_model_id: String,
    pub is_active: Option<bool>,
    pub priority: Option<i32>,
    pub protocols: String,
    pub status: Option<String>,
    pub timeout: Option<i32>,
    pub context_length: Option<i32>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct UpdateProviderModelMapRequest {
    pub provider_model_id: Option<String>,
    pub is_active: Option<bool>,
    pub priority: Option<i32>,
    pub protocols: Option<String>,
    pub status: Option<String>,
    pub timeout: Option<i32>,
    pub context_length: Option<i32>,
}

impl From<CreateProviderModelMapRequest> for crate::db::impls::provider_model_map::ProviderModelMapInput {
    fn from(req: CreateProviderModelMapRequest) -> Self {
        crate::db::impls::provider_model_map::ProviderModelMapInput {
            model_id: req.model_id,
            provider_id: req.provider_id,
            provider_model_id: req.provider_model_id,
            is_active: req.is_active.unwrap_or(true),
            priority: req.priority.unwrap_or(0),
            protocols: req.protocols,
            status: req.status.unwrap_or_else(|| "available".to_string()),
            timeout: req.timeout,
            context_length: req.context_length,
        }
    }
}

impl From<UpdateProviderModelMapRequest> for crate::db::impls::provider_model_map::ProviderModelMapUpdate {
    fn from(req: UpdateProviderModelMapRequest) -> Self {
        crate::db::impls::provider_model_map::ProviderModelMapUpdate {
            provider_model_id: req.provider_model_id,
            is_active: req.is_active,
            priority: req.priority,
            protocols: req.protocols,
            status: req.status,
            timeout: req.timeout,
            context_length: req.context_length,
        }
    }
}

#[derive(Deserialize)]
pub struct ListProviderModelMapsQuery {
    pub model_id: Option<i32>,
    pub provider_id: Option<i32>,
}

#[derive(Serialize)]
pub struct ProviderModelMapResponse {
    pub id: i32,
    pub model_id: i32,
    pub provider_id: i32,
    pub provider_model_id: String,
    pub is_active: bool,
    pub priority: i32,
    pub context_length: Option<i32>,
    pub protocols: String,
    pub status: String,
    pub timeout: Option<i32>,
    pub created_time: chrono::NaiveDateTime,
    pub last_updated: chrono::NaiveDateTime,
}

impl From<crate::db::entities::provider_model_map::Model> for ProviderModelMapResponse {
    fn from(m: crate::db::entities::provider_model_map::Model) -> Self {
        ProviderModelMapResponse {
            id: m.id,
            model_id: m.model_id,
            provider_id: m.provider_id,
            provider_model_id: m.provider_model_id,
            is_active: m.is_active,
            priority: m.priority,
            context_length: m.context_length,
            protocols: m.protocols,
            status: m.status,
            timeout: m.timeout,
            created_time: m.created_time,
            last_updated: m.last_updated,
        }
    }
}

pub async fn list_provider_model_maps(
    state: web::Data<AppState>,
    query: web::Query<ListProviderModelMapsQuery>,
) -> HttpResponse {
    match state.database.provider_model_maps.list_filtered(query.model_id, query.provider_id).await {
        Ok(list) => {
            let results: Vec<ProviderModelMapResponse> = list.into_iter().map(Into::into).collect();
            HttpResponse::Ok().json(results)
        }
        Err(e) => {
            log::error!("{}", e);
            response::db_error()
        }
    }
}

pub async fn create_provider_model_map(
    state: web::Data<AppState>,
    body: web::Json<CreateProviderModelMapRequest>,
) -> HttpResponse {
    let input: crate::db::impls::provider_model_map::ProviderModelMapInput = body.into_inner().into();
    handle_result!(state.database.provider_model_maps.create(&input).await.map(ProviderModelMapResponse::from))
}

pub async fn update_provider_model_map(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<UpdateProviderModelMapRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    let update: crate::db::impls::provider_model_map::ProviderModelMapUpdate = body.into_inner().into();
    handle_result!(state.database.provider_model_maps.update(id, &update).await.map(ProviderModelMapResponse::from))
}

pub async fn delete_provider_model_map(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_delete!(state.database.provider_model_maps.delete(path.into_inner()).await, "Provider model map not found")
}

pub async fn get_provider_model_map(state: web::Data<AppState>, path: web::Path<i32>) -> HttpResponse {
    handle_find!(
        state.database.provider_model_maps.get(path.into_inner()).await
            .map(|opt| opt.map(ProviderModelMapResponse::from)),
        "Provider model map not found"
    )
}
