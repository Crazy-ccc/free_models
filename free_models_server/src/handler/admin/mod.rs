use actix_web::web;

macro_rules! handle_result {
    ($expr:expr) => {
        match $expr {
            Ok(v) => HttpResponse::Ok().json(v),
            Err(e) => {
                log::error!("{}", e);
                response::from_store_error(e)
            }
        }
    };
}

macro_rules! handle_find {
    ($expr:expr, $not_found:expr) => {
        match $expr {
            Ok(Some(v)) => HttpResponse::Ok().json(v),
            Ok(None) => response::not_found($not_found),
            Err(e) => {
                log::error!("{}", e);
                response::from_store_error(e)
            }
        }
    };
}

macro_rules! handle_delete {
    ($expr:expr, $not_found:expr) => {
        match $expr {
            Ok(true) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
            Ok(false) => response::not_found($not_found),
            Err(e) => {
                log::error!("{}", e);
                response::from_store_error(e)
            }
        }
    };
}

pub mod api_key;
pub mod import_models;
pub mod model;
pub mod provider;
pub mod provider_credential;
pub mod provider_model_map;
pub mod stats;
pub mod test_credential;

pub fn admin_routes() -> actix_web::Scope {
    web::scope("/admin")
        .route("/service/status", web::get().to(stats::get_service_status))
        .route("/cache/refresh", web::post().to(stats::refresh_cache))
        .route("/providers", web::get().to(provider::list_providers))
        .route("/providers", web::post().to(provider::create_provider))
        .route("/providers/{id}", web::get().to(provider::get_provider))
        .route("/providers/{id}", web::put().to(provider::update_provider))
        .route("/providers/{id}", web::delete().to(provider::delete_provider))
        .route("/providers/{id}/models", web::get().to(provider::get_provider_models))
        .route("/providers/{id}/models/import", web::post().to(import_models::import_provider_models))
        .route("/models", web::get().to(model::list_models))
        .route("/models", web::post().to(model::create_model))
        .route("/models/{id}", web::get().to(model::get_model))
        .route("/models/{id}", web::put().to(model::update_model))
        .route("/models/{id}", web::delete().to(model::delete_model))
        .route("/provider_model_maps", web::get().to(provider_model_map::list_provider_model_maps))
        .route("/provider_model_maps", web::post().to(provider_model_map::create_provider_model_map))
        .route("/provider_model_maps/{id}", web::get().to(provider_model_map::get_provider_model_map))
        .route("/provider_model_maps/{id}", web::put().to(provider_model_map::update_provider_model_map))
        .route("/provider_model_maps/{id}", web::delete().to(provider_model_map::delete_provider_model_map))
        .route("/api_keys", web::get().to(api_key::list_api_keys))
        .route("/api_keys", web::post().to(api_key::create_api_key))
        .route("/api_keys/{id}", web::get().to(api_key::get_api_key))
        .route("/api_keys/{id}", web::put().to(api_key::update_api_key))
        .route("/api_keys/{id}", web::delete().to(api_key::delete_api_key))
        .route("/provider_credentials", web::get().to(provider_credential::list_provider_credentials))
        .route("/provider_credentials", web::post().to(provider_credential::create_provider_credential))
        .route("/provider_credentials/{id}", web::get().to(provider_credential::get_provider_credential))
        .route("/provider_credentials/{id}", web::put().to(provider_credential::update_provider_credential))
        .route("/provider_credentials/{id}", web::delete().to(provider_credential::delete_provider_credential))
        .route("/test_credential", web::post().to(test_credential::test_credential))
        .route("/usage_log/stats", web::get().to(stats::usage_log_stats))
}
