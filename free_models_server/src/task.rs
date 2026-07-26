use std::time::Duration;

use crate::db::impls::UsageLogStoreSeaorm;

use crate::util::usage_log_collector::UsageLogCollector;

pub(crate) async fn graceful_shutdown(
    server_handle: actix_web::dev::ServerHandle,
    log_collector: UsageLogCollector,
) {
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
    log::info!("Shutting down gracefully...");
    match tokio::time::timeout(Duration::from_secs(30), server_handle.stop(true)).await {
        Ok(_) => {}
        Err(_) => {
            log::info!("Graceful shutdown timed out, forcing exit");
            server_handle.stop(false).await;
        }
    }
    log_collector.shutdown().await;
    log::info!("Usage log collector flushed");
}

pub(crate) fn spawn_daily_archive(usage_log_store: UsageLogStoreSeaorm) {
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

            let mut backoff_secs = 1u64;
            loop {
                match usage_log_store.archive_yesterday().await {
                    Ok(_) => break,
                    Err(e) => {
                        log::warn!("Daily archive failed: {}", e);
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(3600);
                    }
                }
            }
        }
    });
}
