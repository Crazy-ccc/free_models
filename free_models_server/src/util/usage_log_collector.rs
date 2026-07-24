use std::sync::OnceLock;
use std::time::Duration;
use sea_orm::DatabaseConnection;
use tokio::sync::mpsc;

use crate::service::model_service::ModelProviderInfo;
use crate::service::usage_log_service;
use crate::util::proxy_types::{Protocol, UsageInfo};

static USAGE_LOG_SENDER: OnceLock<mpsc::Sender<usage_log_service::UsageLogInsert>> = OnceLock::new();

/// 后台收集 usage_log 并批量写入
pub struct UsageLogCollector {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    handle: tokio::task::JoinHandle<()>,
}

impl UsageLogCollector {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(5), self.handle).await;
    }
}

/// 初始化 usage_log 批量收集器，在 main.rs 启动时调用
pub fn init_usage_log_collector(db: DatabaseConnection) -> UsageLogCollector {
    let (tx, rx) = mpsc::channel::<usage_log_service::UsageLogInsert>(256);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    USAGE_LOG_SENDER
        .set(tx.clone())
        .expect("UsageLogCollector already initialized");
    let handle = tokio::spawn(run_collector(db, rx, shutdown_rx));
    UsageLogCollector { shutdown_tx, handle }
}

async fn run_collector(
    db: DatabaseConnection,
    mut rx: mpsc::Receiver<usage_log_service::UsageLogInsert>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    let batch_size = 10;
    let mut buffer = Vec::with_capacity(batch_size);
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.tick().await;

    loop {
        tokio::select! {
            record = rx.recv() => {
                match record {
                    Some(r) => buffer.push(r),
                    None => break,
                }
                if buffer.len() >= batch_size {
                    if let Err(e) = usage_log_service::create_batch(&db, &buffer).await {
                        log::error!("Failed to batch insert usage logs: {}", e);
                    }
                    buffer.clear();
                }
            }
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    if let Err(e) = usage_log_service::create_batch(&db, &buffer).await {
                        log::error!("Failed to batch insert usage logs: {}", e);
                    }
                    buffer.clear();
                }
            }
            _ = shutdown_rx.changed() => break,
        }
    }

    if !buffer.is_empty() {
        if let Err(e) = usage_log_service::create_batch(&db, &buffer).await {
            log::error!("Failed to batch insert usage logs: {}", e);
        }
    }
}

pub(crate) fn spawn_usage_log(
    model_info: &ModelProviderInfo,
    api_key_id: Option<i32>,
    api_key_name: Option<&str>,
    protocol: Protocol,
    duration_ms: i32,
    is_stream: bool,
    info: UsageInfo,
    status: &str,
    error_message: Option<&str>,
) {
    let record = usage_log_service::UsageLogInsert {
        api_key_id,
        api_key_name: api_key_name.map(|s| s.to_string()),
        model_config_id: model_info.model_config_id,
        provider_config_id: model_info.provider_config_id,
        provider_credential_id: model_info.provider_credential_id,
        model_name: model_info.model_name.clone(),
        provider_name: model_info.provider_name.clone(),
        protocol: protocol.as_str().to_string(),
        status: status.to_string(),
        error_message: error_message.map(|s| s.to_string()),
        prompt_tokens: info.prompt_tokens,
        completion_tokens: info.completion_tokens,
        total_tokens: info.total_tokens,
        cache_hit_tokens: info.cache_hit_tokens,
        cache_miss_tokens: info.cache_miss_tokens,
        duration_ms,
        is_stream,
    };

    if let Some(tx) = USAGE_LOG_SENDER.get() {
        if tx.try_send(record).is_err() {
            log::warn!("Usage log channel full, dropping record");
        }
    } else {
        log::warn!("Usage log collector not initialized, dropping record");
    }
}
