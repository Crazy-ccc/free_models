use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::response;
use crate::AppState;

#[derive(Deserialize)]
pub struct ImportModelItem {
    pub model_id: String,
    pub provider_model_id: String,
    pub protocols: Option<String>,
    pub context_length: Option<i32>,
}

async fn import_one(
    state: &AppState,
    provider_id: i32,
    item: &ImportModelItem,
) -> Result<serde_json::Value, String> {
    let model_name = &item.model_id;

    // a. Look up model_config by name; do NOT auto-create. Import = mapping only.
    let model = match state.database.model_configs.find_by_name(model_name).await {
        Ok(Some(m)) => m,
        Ok(None) => return Err(format!("Model '{}' not found, please create it first", model_name)),
        Err(e) => return Err(format!("DB error for '{}': {}", model_name, e)),
    };

    // b. Create or update provider_model_map
    let existing_map = state.database.provider_model_maps.find_by_model_and_provider(model.id, provider_id).await;
    match existing_map {
        Ok(Some(map)) => {
            let update = crate::db::impls::provider_model_map::ProviderModelMapUpdate {
                provider_model_id: Some(item.provider_model_id.clone()),
                protocols: Some(item.protocols.clone().unwrap_or_else(|| "openai".to_string())),
                context_length: item.context_length,
                ..Default::default()
            };
            if let Err(e) = state.database.provider_model_maps.update(map.id, &update).await {
                return Err(format!("Failed to update map for '{}': {}", model_name, e));
            }
        }
        Ok(None) => {
            let input = crate::db::impls::provider_model_map::ProviderModelMapInput {
                model_id: model.id,
                provider_id,
                provider_model_id: item.provider_model_id.clone(),
                is_active: true,
                priority: 9,
                protocols: item.protocols.clone().unwrap_or_else(|| "openai".to_string()),
                status: "available".to_string(),
                timeout: Some(30),
                context_length: item.context_length.or(Some(256000)),
            };
            if let Err(e) = state.database.provider_model_maps.create(&input).await {
                return Err(format!("Failed to create map for '{}': {}", model_name, e));
            }
        }
        Err(e) => return Err(format!("DB error for '{}': {}", model_name, e)),
    }

    Ok(serde_json::json!({
        "name": model_name,
        "model_config_id": model.id,
        "provider_model_id": &item.provider_model_id,
    }))
}

pub async fn import_provider_models(
    state: web::Data<AppState>,
    path: web::Path<i32>,
    body: web::Json<Vec<ImportModelItem>>,
) -> HttpResponse {
    let provider_id = path.into_inner();

    // Verify provider exists
    if let Ok(None) = state.database.provider_configs.get(provider_id).await {
        return response::not_found("Provider not found");
    }

    let mut imported: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for item in body.iter() {
        match import_one(&state, provider_id, item).await {
            Ok(v) => imported.push(v),
            Err(e) => errors.push(e),
        }
    }

    HttpResponse::Ok().json(serde_json::json!({
        "imported": imported,
        "errors": errors,
    }))
}
