use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::db::types::UsageLogInsert;
use crate::db::impls::UsageLogStoreSeaorm;

use crate::util::model_scheduler::{CredentialInfo, ModelProviderMap, ModelScheduleInfo};
use crate::util::proxy_types::{ApiKeyContext, LogContext};

static USAGE_LOG_SENDER: OnceLock<mpsc::Sender<UsageLogInsert>> = OnceLock::new();

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
pub fn init_usage_log_collector(store: UsageLogStoreSeaorm) -> UsageLogCollector {
    let (tx, rx) = mpsc::channel::<UsageLogInsert>(256);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    USAGE_LOG_SENDER
        .set(tx.clone())
        .expect("UsageLogCollector already initialized");
    let handle = tokio::spawn(run_collector(store, rx, shutdown_rx));
    UsageLogCollector { shutdown_tx, handle }
}

async fn flush_buffer(store: &UsageLogStoreSeaorm, buf: &mut Vec<UsageLogInsert>) {
    if buf.is_empty() {
        return;
    }
    if let Err(e) = store.create_batch(buf).await {
        log::error!("Failed to batch insert usage logs: {}", e);
    }
    buf.clear();
}

async fn run_collector(
    store: UsageLogStoreSeaorm,
    mut rx: mpsc::Receiver<UsageLogInsert>,
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
                    flush_buffer(&store, &mut buffer).await;
                }
            }
            _ = interval.tick() => {
                flush_buffer(&store, &mut buffer).await;
            }
            _ = shutdown_rx.changed() => break,
        }
    }

    flush_buffer(&store, &mut buffer).await;
}

pub(crate) fn spawn_usage_log(
    model_info: &ModelScheduleInfo,
    map: &ModelProviderMap,
    cred: &CredentialInfo,
    api_key_ctx: &ApiKeyContext,
    log_ctx: &LogContext,
) {
    let record = UsageLogInsert {
        api_key_id: api_key_ctx.id,
        api_key_name: api_key_ctx.name.clone(),
        model_config_id: model_info.model_config_id,
        provider_config_id: map.provider_config_id,
        provider_credential_id: cred.provider_credential_id,
        model_name: model_info.model_name.clone(),
        provider_name: map.provider_name.clone(),
        protocol: log_ctx.protocol.as_str().to_string(),
        status: log_ctx.status.clone(),
        error_message: log_ctx.error_message.clone(),
        prompt_tokens: log_ctx.info.prompt_tokens,
        completion_tokens: log_ctx.info.completion_tokens,
        total_tokens: log_ctx.info.total_tokens,
        cache_hit_tokens: log_ctx.info.cache_hit_tokens,
        cache_miss_tokens: log_ctx.info.cache_miss_tokens,
        duration_ms: log_ctx.duration_ms,
        is_stream: log_ctx.is_stream,
    };

    if let Some(tx) = USAGE_LOG_SENDER.get() {
        if tx.try_send(record).is_err() {
            log::warn!("Usage log channel full, dropping record");
        }
    } else {
        log::warn!("Usage log collector not initialized, dropping record");
    }
}
