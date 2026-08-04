use std::net::IpAddr;
use std::time::Duration;

use moka::sync::Cache;

fn extract_host(url: &str) -> Result<&str, String> {
    let rest = if let Some(r) = url.strip_prefix("https://") {
        r
    } else if let Some(r) = url.strip_prefix("http://") {
        r
    } else {
        return Err("Only http and https protocols are allowed".to_string());
    };

    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.split(':').next().unwrap_or(host);

    if host.is_empty() {
        return Err("URL has no host".to_string());
    }

    Ok(host)
}

pub(crate) async fn check_url_safe(url: &str) -> Result<(), String> {
    let host = extract_host(url)?;

    let addrs = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| format!("DNS resolution failed: {}", e))?;
    for addr in addrs {
        if is_private_ip(addr.ip()) {
            return Err(format!("Target URL resolves to a private IP: {}", addr.ip()));
        }
    }

    Ok(())
}

pub(crate) async fn validate_url_safe(url: &str) -> Result<(), String> {
    check_url_safe(url).await
}

pub struct SsrfChecker {
    cache: Cache<String, bool>,
}

impl SsrfChecker {
    pub fn new(max_capacity: u64, ttl: Duration) -> Self {
        SsrfChecker {
            cache: Cache::builder()
                .max_capacity(max_capacity)
                .time_to_live(ttl)
                .build(),
        }
    }

    pub async fn validate_url_safe(&self, url: &str) -> Result<(), String> {
        let host = extract_host(url)?;

        if let Some(safe) = self.cache.get(host) {
            return if safe {
                Ok(())
            } else {
                Err("Target URL is not safe".to_string())
            };
        }

        let result = check_url_safe(url).await;
        self.cache.insert(host.to_string(), result.is_ok());
        result
    }
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}