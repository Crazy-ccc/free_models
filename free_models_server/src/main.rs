mod config;
mod db;
mod error;
mod handler;
mod middleware;
mod service;
mod util;

use std::time::Duration;

use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use log::info;
use reqwest::Client;
use sea_orm::DatabaseConnection;

use crate::middleware::admin_auth::AdminAuthMiddleware;
use crate::middleware::auth::AuthMiddleware;
use util::api_key_cache::ApiKeyCache;
use crate::service::model_service::{ModelCache, ProviderCache};
use util::penalty::PriorityPenalty;
use config::Config;

pub struct AppState {
    pub db: DatabaseConnection,
    pub model_cache: ModelCache,
    pub provider_cache: ProviderCache,
    pub client: Client,
    pub priority_penalty: web::Data<PriorityPenalty>,
    pub redis: db::redis::RedisManager,
    pub api_key_cache: ApiKeyCache,
    pub encryption_key: [u8; 32],
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = Config::from_env();

    let db = db::init_db(&config.database_url, config.db_max_connections).await;

    let redis_manager = if config.redis_enabled {
        db::redis::RedisManager::new(&config.redis_url).await
    } else {
        db::redis::RedisManager::disabled()
    };

    info!(
        "Starting server at {}:{}",
        config.server_host, config.server_port
    );

    info!("Auto-filling fingerprints for admin_key table");
    service::admin_key_service::auto_fill_fingerprints(&db).await;

    let api_key_cache = ApiKeyCache::load_all(&db, redis_manager.clone()).await;
    info!("API key cache loaded");

    let client = Client::builder()
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    let priority_penalty = web::Data::new(PriorityPenalty::new(redis_manager.clone(), config.redis_cache_ttl_penalty));
    let app_state = web::Data::new(AppState {
        db: db.clone(),
        model_cache: ModelCache::new(redis_manager.clone(), config.redis_cache_ttl_model),
        provider_cache: ProviderCache::new(redis_manager.clone(), config.redis_cache_ttl_provider),
        client,
        priority_penalty: priority_penalty.clone(),
        redis: redis_manager.clone(),
        api_key_cache: api_key_cache.clone(),
        encryption_key: config.encryption_key,
    });
    let server = HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .app_data(priority_penalty.clone())
            .wrap(AuthMiddleware)
            .wrap(actix_middleware::Logger::default())
            .route("/health", web::get().to(handler::chat_handler::health_check))
            .route("/v1/models", web::get().to(handler::chat_handler::list_models))
            .route("/v1/chat/completions", web::post().to(handler::chat_handler::chat_completions))
            .route("/v1/messages", web::post().to(handler::chat_handler::anthropic_messages))
            .service(
                handler::admin_handler::admin_routes().wrap(AdminAuthMiddleware),
            )
    })
    .bind(format!("{}:{}", config.server_host, config.server_port))?
    .run();

    let server_handle = server.handle();

    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.expect("Failed to install Ctrl-C handler");
        }
        info!("Shutting down gracefully...");
        match tokio::time::timeout(Duration::from_secs(30), server_handle.stop(true)).await {
            Ok(_) => {}
            Err(_) => {
                info!("Graceful shutdown timed out, forcing exit");
                server_handle.stop(false).await;
            }
        }
    });

    server.await?;
    Ok(())
}
