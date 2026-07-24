use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "provider_credential")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub provider_id: i32,
    pub name: String,
    pub api_key: String,
    pub account: Option<String>,
    pub encrypted_password: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub created_time: DateTime,
    pub last_updated: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::provider_config::Entity",
        from = "Column::ProviderId",
        to = "super::provider_config::Column::Id"
    )]
    ProviderConfig,
}

impl Related<super::provider_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProviderConfig.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
