#[derive(Clone)]
pub struct UsageLogInsert {
    pub api_key_id: Option<i32>,
    pub api_key_name: Option<String>,
    pub model_config_id: i32,
    pub provider_config_id: i32,
    pub provider_credential_id: i32,
    pub model_name: String,
    pub provider_name: String,
    pub protocol: String,
    pub status: String,
    pub error_message: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub cache_miss_tokens: i32,
    pub duration_ms: i32,
    pub is_stream: bool,
}

#[derive(serde::Serialize)]
pub struct UsageLogStatItem {
    pub dimension: String,
    pub dimension_id: Option<String>,
    pub dimension_name: String,
    pub requests: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cache_hit_tokens: i64,
    pub cache_miss_tokens: i64,
    pub avg_duration_ms: f64,
    pub min_duration_ms: i32,
    pub max_duration_ms: i32,
}

#[derive(serde::Serialize)]
pub struct UsageLogStatsResponse {
    pub total: UsageLogStatItem,
    pub items: Vec<UsageLogStatItem>,
}
