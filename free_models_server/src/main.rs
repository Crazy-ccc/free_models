mod app;
mod cache;
mod config;
mod db;
mod handler;
mod middleware;
mod response;
mod service;
mod task;
mod util;

use std::sync::Arc;

use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use log::info;

use crate::middleware::admin_auth::AdminAuthMiddleware;
use crate::middleware::auth::AuthMiddleware;
use util::api_key_cache::ApiKeyCache;
use util::cache_affinity::CacheAffinity;
use util::model_scheduler::SchedulerCache;
use util::penalty::CircuitBreaker;
use config::Config;

use crate::app::AppState;
use crate::util::usage_log_collector::init_usage_log_collector;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let config = Config::from_env();

    let database = db::build_database().await;
    let usage_log_store = database.usage_logs.clone();

    info!(
        "Starting server at {}:{}",
        config.server_host, config.server_port
    );

    let cache_store: Arc<cache::RedisManager> = Arc::new(cache::RedisManager::init().await);
    let api_key_cache = ApiKeyCache::load_all(&database.api_keys, cache_store.clone(), config.api_key_cache_max_capacity).await;
    info!("API key cache loaded");

    let client = app::build_client();

    let priority_penalty = web::Data::new(CircuitBreaker::new(
        config.circuit_breaker_open_ttl,
        config.circuit_breaker_threshold,
        config.circuit_breaker_max_capacity,
    ));
    let cache_affinity = CacheAffinity::new(
        config.cache_affinity_max_capacity,
        config.cache_affinity_ttl,
    );
    let app_state = web::Data::new(AppState {
        database,
        scheduler_cache: SchedulerCache::new(cache_store, config.redis_cache_ttl_model),
        client,
        priority_penalty: priority_penalty.clone(),
        api_key_cache,
        cache_affinity,
        encryption_key: config.encryption_key,
    });
    let log_collector = init_usage_log_collector(usage_log_store.clone());

    if let Err(e) = usage_log_store.archive_yesterday().await {
        log::warn!("Failed to archive yesterday's usage log: {}", e);
    }

    let server = HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .app_data(priority_penalty.clone())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .wrap(AuthMiddleware)
            .wrap(actix_middleware::Logger::default())
            .route("/health", web::get().to(handler::chat_handler::health_check))
            .route("/v1/models", web::get().to(handler::chat_handler::list_models))
            .route("/v1/chat/completions", web::post().to(handler::chat_handler::chat_completions))
            .route("/v1/messages", web::post().to(handler::chat_handler::anthropic_messages))
            .route("/v1/responses", web::post().to(handler::chat_handler::responses_messages))
            .service(
                handler::admin::admin_routes().wrap(AdminAuthMiddleware),
            )
    })
    .bind(format!("{}:{}", config.server_host, config.server_port))?
    .run();

    let server_handle = server.handle();

    tokio::spawn(task::graceful_shutdown(server_handle, log_collector));
    task::spawn_daily_archive(usage_log_store);

    server.await?;
    Ok(())
}
