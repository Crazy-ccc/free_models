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

    pub fn list_blocked_models(&self) -> Vec<(String, String, i32, u64)> {
        let now = Instant::now();
        let mut result = Vec::new();

        for (key, entry) in self.entries.iter() {
            if let Some(expiry) = entry.fallback {
                if expiry > now {
                    let remaining = expiry.duration_since(now).as_secs();
                    let parts: Vec<&str> = key.splitn(3, '|').collect();
                    if parts.len() == 3 {
                        let model_name = parts[0].to_string();
                        let provider_name = parts[1].to_string();
                        let credential_id = parts[2].parse().unwrap_or(0);
                        result.push((model_name, provider_name, credential_id, remaining));
                    }
                }
            }
        }

        result
    }
}
