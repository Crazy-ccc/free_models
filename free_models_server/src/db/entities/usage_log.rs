use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub api_key_id: Option<i32>,
    pub api_key_name: Option<String>,
    pub model_config_id: Option<i32>,
    pub provider_config_id: Option<i32>,
    pub provider_credential_id: Option<i32>,
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
    pub request_timestamp: chrono::NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
