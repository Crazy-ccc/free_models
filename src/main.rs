mod config;
mod db;
mod error;
mod handler;
mod middleware;
mod service;

use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use log::info;
use reqwest::Client;
use sea_orm::DatabaseConnection;
use std::time::Duration;

use crate::middleware::auth::AuthMiddleware;
use crate::service::api_key_cache::ApiKeyCache;
use crate::service::model_service::{ModelCache, ProviderCache};
use crate::service::penalty::PriorityPenalty;
use config::Config;

pub struct AppState {
    pub db: DatabaseConnection,
    pub model_cache: ModelCache,
    pub provider_cache: ProviderCache,
    pub client: Client,
    pub priority_penalty: web::Data<PriorityPenalty>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = Config::from_env();

    let db = db::init_db(&config.database_url, config.db_max_connections).await;

    info!(
        "Starting server at {}:{}",
        config.server_host, config.server_port
    );

    let api_key_cache = ApiKeyCache::load_all(&db).await;
    info!("API key cache loaded");

    let client = Client::new();
    let priority_penalty = web::Data::new(PriorityPenalty::new());
    let app_state = web::Data::new(AppState {
        db: db.clone(),
        model_cache: ModelCache::new(),
        provider_cache: ProviderCache::new(),
        client,
        priority_penalty: priority_penalty.clone(),
    });
    let db_data = web::Data::new(db.clone());
    let api_key_data = web::Data::new(api_key_cache);

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .app_data(db_data.clone())
            .app_data(api_key_data.clone())
            .app_data(priority_penalty.clone())
            .wrap(AuthMiddleware)
            .wrap(actix_middleware::Logger::default())
            .route("/v1/models", web::get().to(handler::chat_handler::list_models))
            .route("/v1/chat/completions", web::post().to(handler::chat_handler::chat_completions))
    })
    .bind(format!("{}:{}", config.server_host, config.server_port))?
    .run()
    .await
}