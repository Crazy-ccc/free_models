mod app;
mod config;
mod db;
mod handler;
mod middleware;
mod response;
mod service;
mod util;

use std::time::Duration;

use actix_web::{web, App, HttpServer, middleware as actix_middleware};
use log::info;

use crate::middleware::admin_auth::AdminAuthMiddleware;
use crate::middleware::auth::AuthMiddleware;
use util::api_key_cache::ApiKeyCache;
use util::model_scheduler::SchedulerCache;
use util::penalty::PriorityPenalty;
use config::Config;

use crate::app::AppState;
use crate::util::usage_log_collector::init_usage_log_collector;

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

    let api_key_cache = ApiKeyCache::load_all(&db, redis_manager.clone()).await;
    info!("API key cache loaded");

    let client = app::build_client();

    let priority_penalty = web::Data::new(PriorityPenalty::new(redis_manager.clone(), config.redis_cache_ttl_penalty));
    let app_state = web::Data::new(AppState {
        db: db.clone(),
        scheduler_cache: SchedulerCache::new(redis_manager.clone(), config.redis_cache_ttl_model),
        client,
        priority_penalty: priority_penalty.clone(),
        redis: redis_manager.clone(),
        api_key_cache: api_key_cache.clone(),
        encryption_key: config.encryption_key,
    });
    let log_collector = init_usage_log_collector(db.clone());

    // 启动时归档昨日数据
    if let Err(e) = service::usage_log_service::archive_yesterday(&db).await {
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
        log_collector.shutdown().await;
        info!("Usage log collector flushed");
    });

    // 每日定时归档（00:05 UTC 执行）
    let archive_db = db.clone();
    tokio::spawn(async move {
        loop {
            let now = chrono::Utc::now();
            let next_date = now.date_naive().succ_opt().unwrap_or_else(|| now.date_naive());
            let next_naive = match next_date.and_hms_opt(0, 5, 0) {
                Some(t) => t,
                None => {
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    continue;
                }
            };
            let now_naive = now.naive_utc();
            let secs_until = (next_naive - now_naive).num_seconds().max(60) as u64;
            tokio::time::sleep(Duration::from_secs(secs_until)).await;

            if let Err(e) = service::usage_log_service::archive_yesterday(&archive_db).await {
                log::warn!("Daily archive failed: {}", e);
            }
        }
    });

    server.await?;
    Ok(())
}
