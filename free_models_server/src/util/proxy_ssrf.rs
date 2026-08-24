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
#[cfg(test)]
mod ssrf_tests {
    use super::*;

    #[test]
    fn extract_host_strips_scheme_port_path() {
        assert_eq!(extract_host("https://api.example.com:8443/v1/x?q=1").unwrap(), "api.example.com");
        assert_eq!(extract_host("http://host/a/b").unwrap(), "host");
    }

    #[test]
    fn extract_host_rejects_other_schemes() {
        assert!(extract_host("ftp://example.com").is_err());
        assert!(extract_host("file:///etc/passwd").is_err());
    }

    #[test]
    fn extract_host_rejects_empty() {
        assert!(extract_host("http://").is_err());
    }

    #[test]
    fn private_ip_classes_detected() {
        for s in ["127.0.0.1", "10.1.2.3", "192.168.0.9", "172.16.5.5", "169.254.3.4", "0.0.0.0", "255.255.255.255", "::1"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(is_private_ip(ip), "should be private: {}", s);
        }
    }

    #[test]
    fn public_ip_allowed() {
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
    }

    #[tokio::test]
    async fn rejects_non_http_scheme_before_dns() {
        assert!(validate_url_safe("ftp://example.com").await.is_err());
    }

    #[tokio::test]
    async fn blocks_private_literal_ips() {
        for url in ["http://127.0.0.1/", "http://10.0.0.2:8080/api", "http://192.168.1.1", "http://169.254.9.9/x"] {
            assert!(validate_url_safe(url).await.is_err(), "should be blocked: {}", url);
        }
    }

    #[tokio::test]
    async fn allows_public_numeric_ip() {
        assert!(validate_url_safe("http://8.8.8.8/dns-query").await.is_ok());
    }
}
