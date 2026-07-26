use std::time::Duration;

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub provider_name: String,
    pub provider_credential_id: i32,
}

#[derive(Clone)]
pub struct CacheAffinity {
    records: moka::sync::Cache<(i32, String), CacheEntry>,
}

impl CacheAffinity {
    pub fn new(max_capacity: u64, ttl: Duration) -> Self {
        CacheAffinity {
            records: moka::sync::Cache::builder()
                .max_capacity(max_capacity)
                .time_to_live(ttl)
                .build(),
        }
    }

    /// 记录某个 API Key + Model 成功转发的目标
    pub fn record_success(
        &self,
        api_key_id: i32,
        model_name: &str,
        provider_name: &str,
        provider_credential_id: i32,
    ) {
        self.records.insert(
            (api_key_id, model_name.to_string()),
            CacheEntry {
                provider_name: provider_name.to_string(),
                provider_credential_id,
            },
        );
    }

    /// 获取某个 API Key + Model 的亲和性记录
    pub fn get_affinity(&self, api_key_id: i32, model_name: &str) -> Option<CacheEntry> {
        self.records.get(&(api_key_id, model_name.to_string()))
    }

    /// 统计当前有多少个 API Key 关联到指定 credential
    pub fn count_by_credential(&self, provider_credential_id: i32) -> usize {
        self.records
            .iter()
            .filter(|(_, e)| e.provider_credential_id == provider_credential_id)
            .count()
    }
}
