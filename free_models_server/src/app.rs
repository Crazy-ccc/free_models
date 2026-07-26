use std::time::Duration;

use actix_web::web;
use reqwest::Client;

use crate::db::Database;
use crate::util::api_key_cache::ApiKeyCache;
use crate::util::cache_affinity::CacheAffinity;
use crate::util::model_scheduler::SchedulerCache;
use crate::util::penalty::CircuitBreaker;

pub struct AppState {
    pub database: Database,
    pub scheduler_cache: SchedulerCache,
    pub client: Client,
    pub priority_penalty: web::Data<CircuitBreaker>,
    pub api_key_cache: ApiKeyCache,
    pub cache_affinity: CacheAffinity,
    pub encryption_key: [u8; 32],
}

pub fn build_client() -> Client {
    Client::builder()
        .pool_max_idle_per_host(20)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_keepalive(Duration::from_secs(30))
        .user_agent("FreeModelsServer/1.0")
        .build()
        .expect("Failed to build HTTP client")
}
