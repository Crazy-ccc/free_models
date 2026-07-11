use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_PRIORITY_PENALTY_TTL: Duration = Duration::from_secs(1800);

pub struct PriorityPenalty {
    penalties: Mutex<HashMap<String, Instant>>,
    ttl: Duration,
}

impl PriorityPenalty {
    pub fn new() -> Self {
        PriorityPenalty {
            penalties: Mutex::new(HashMap::new()),
            ttl: DEFAULT_PRIORITY_PENALTY_TTL,
        }
    }

    fn key(model_name: &str, provider_name: &str) -> String {
        format!("{}|{}", model_name, provider_name)
    }

    pub fn penalize(&self, model_name: &str, provider_name: &str) {
        let key = Self::key(model_name, provider_name);
        let mut penalties = self.penalties.lock().unwrap();
        penalties.insert(key, Instant::now() + self.ttl);
    }

    pub fn is_penalized(&self, model_name: &str, provider_name: &str) -> bool {
        let key = Self::key(model_name, provider_name);
        let mut penalties = self.penalties.lock().unwrap();
        match penalties.get(&key) {
            Some(expiry) => {
                if *expiry > Instant::now() {
                    true
                } else {
                    penalties.remove(&key);
                    false
                }
            }
            None => false,
        }
    }

    pub fn sort_penalized_last<T, F>(&self, items: Vec<T>, key_of: F) -> Vec<T>
    where
        F: Fn(&T) -> (String, String),
    {
        let mut penalized: Vec<T> = Vec::new();
        let mut rest: Vec<T> = Vec::new();
        for item in items {
            let (model_name, provider_name) = key_of(&item);
            if self.is_penalized(&model_name, &provider_name) {
                penalized.push(item);
            } else {
                rest.push(item);
            }
        }
        rest.extend(penalized);
        rest
    }
}
