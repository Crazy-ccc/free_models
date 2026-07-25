use std::time::Duration;

use actix_web::web;
use reqwest::Client;
use sea_orm::DatabaseConnection;

use crate::db::redis::RedisManager;
use crate::util::api_key_cache::ApiKeyCache;
use crate::util::model_scheduler::SchedulerCache;
use crate::util::penalty::PriorityPenalty;

pub struct AppState {
    pub db: DatabaseConnection,
    pub scheduler_cache: SchedulerCache,
    pub client: Client,
    pub priority_penalty: web::Data<PriorityPenalty>,
    pub redis: RedisManager,
    pub api_key_cache: ApiKeyCache,
    pub encryption_key: [u8; 32],
}

pub fn build_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client")
}
