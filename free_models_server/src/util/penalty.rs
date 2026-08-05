use std::time::{Duration, Instant};


#[derive(Clone, PartialEq, Debug)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone)]
struct CircuitEntry {
    failure_count: u32,
    circuit_state: CircuitState,
    half_open_allowed: bool,
    fallback: Option<Instant>,
}

impl Default for CircuitEntry {
    fn default() -> Self {
        Self {
            failure_count: 0,
            circuit_state: CircuitState::Closed,
            half_open_allowed: false,
            fallback: None,
        }
    }
}

#[derive(Clone)]
pub struct CircuitBreaker {
    open_ttl: Duration,
    threshold: u32,
    entries: moka::sync::Cache<String, CircuitEntry>,
}

impl CircuitBreaker {
    pub fn new(open_ttl: Duration, threshold: u32, max_capacity: u64) -> Self {
        CircuitBreaker {
            open_ttl,
            threshold,
            entries: moka::sync::Cache::builder()
                .max_capacity(max_capacity)
                .build(),
        }
    }

    fn key(model_name: &str, provider_name: &str, credential_id: i32) -> String {
        format!("{}|{}|{}", model_name, provider_name, credential_id)
    }

    /// 解析熔断 key（`model|provider|credential_id`），格式非法或 credential_id 非整数时返回 None。
    fn parse_key(key: &str) -> Option<(&str, &str, i32)> {
        let mut parts = key.splitn(3, '|');
        let model_name = parts.next()?;
        let provider_name = parts.next()?;
        let credential_id = parts.next()?.parse::<i32>().ok()?;
        Some((model_name, provider_name, credential_id))
    }

    pub fn record_failure(&self, model_name: &str, provider_name: &str, credential_id: i32) {
        let key = Self::key(model_name, provider_name, credential_id);
        // 注意：moka 的 get-then-insert 非原子，并发调用可能丢失自增（failure_count 少计）。
        // 对熔断场景可接受——最终仍会达阈值跳闸；如需严格计数可改用 per-key Mutex。
        let mut entry = self.entries.get(&key).unwrap_or_default();
        entry.failure_count += 1;
        if entry.failure_count >= self.threshold {
            let expiry = Instant::now() + self.open_ttl;
            entry.circuit_state = CircuitState::Open;
            entry.fallback = Some(expiry);
        }
        self.entries.insert(key, entry);
    }

    pub fn record_success(&self, model_name: &str, provider_name: &str, credential_id: i32) {
        let key = Self::key(model_name, provider_name, credential_id);
        let mut entry = self.entries.get(&key).unwrap_or_default();
        entry.failure_count = 0;
        entry.circuit_state = CircuitState::Closed;
        entry.half_open_allowed = false;
        entry.fallback = None;
        self.entries.insert(key, entry);
    }

    pub fn is_allowed(&self, model_name: &str, provider_name: &str, credential_id: i32) -> (bool, Option<CircuitState>) {
        let key = Self::key(model_name, provider_name, credential_id);
        let now = Instant::now();

        let Some(mut entry) = self.entries.get(&key) else {
            return (true, Some(CircuitState::Closed));
        };

        match entry.circuit_state {
            CircuitState::Closed => (true, Some(CircuitState::Closed)),
            CircuitState::Open => {
                let expired = match entry.fallback {
                    Some(expiry) => now >= expiry,
                    None => true,
                };
                if expired {
                    entry.circuit_state = CircuitState::HalfOpen;
                    entry.half_open_allowed = true;
                    self.entries.insert(key, entry);
                    (true, Some(CircuitState::HalfOpen))
                } else {
                    (false, Some(CircuitState::Open))
                }
            }
            CircuitState::HalfOpen => {
                if entry.half_open_allowed {
                    // 注意：get-then-insert 非原子，并发调用可能让多个请求同时通过 HalfOpen 探测。
                    // 设计意图为单探测，实际可能多发；对代理熔断可接受，如需严格单探测可改用 per-key Mutex。
                    entry.half_open_allowed = false;
                    self.entries.insert(key, entry);
                    (true, Some(CircuitState::HalfOpen))
                } else {
                    (false, Some(CircuitState::HalfOpen))
                }
            }
        }
    }

    pub fn reset_credential(&self, credential_id: i32) {
        for (key, _) in self.entries.iter() {
            if let Some((_, _, id)) = Self::parse_key(&key)
                && id == credential_id
            {
                self.entries.remove(&**key);
            }
        }
    }

    pub fn list_blocked_models(&self) -> Vec<(String, String, i32, u64)> {
        let now = Instant::now();
        let mut result = Vec::new();

        for (key, entry) in self.entries.iter() {
            if let Some(expiry) = entry.fallback {
                if expiry > now {
                    let remaining = expiry.duration_since(now).as_secs();
                    if let Some((model_name, provider_name, credential_id)) = Self::parse_key(&key) {
                        result.push((model_name.to_string(), provider_name.to_string(), credential_id, remaining));
                    }
                }
            }
        }

        result
    }
}
