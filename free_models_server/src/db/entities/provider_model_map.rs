use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "provider_model_map")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub model_id: i32,
    pub provider_id: i32,
    pub provider_model_id: String,
    pub is_active: bool,
    pub priority: i32,
    pub context_length: Option<i32>,
    pub protocols: String,
    pub status: String,
    pub timeout: Option<i32>,
    pub created_time: DateTime,
    pub last_updated: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_config::Entity",
        from = "Column::ModelId",
        to = "super::model_config::Column::Id"
    )]
    ModelConfig,
    #[sea_orm(
        belongs_to = "super::provider_config::Entity",
        from = "Column::ProviderId",
        to = "super::provider_config::Column::Id"
    )]
    ProviderConfig,
}

impl Related<super::model_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ModelConfig.def()
    }
}

impl Related<super::provider_config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProviderConfig.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
