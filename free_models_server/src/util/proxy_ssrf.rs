use std::net::IpAddr;

/// 校验目标 URL 安全：仅允许 http/https，DNS 解析结果不能是内网 IP
pub(crate) async fn validate_url_safe(url: &str) -> Result<(), String> {
    let (_scheme, rest) = if let Some(r) = url.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("http", r)
    } else {
        return Err("Only http and https protocols are allowed".to_string());
    };

    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.split(':').next().unwrap_or(host);

    if host.is_empty() {
        return Err("URL has no host".to_string());
    }

    if let Ok(addrs) = tokio::net::lookup_host((host, 0)).await {
        for addr in addrs {
            if is_private_ip(addr.ip()) {
                return Err(format!("Target URL resolves to a private IP: {}", addr.ip()));
            }
        }
    }

    Ok(())
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
