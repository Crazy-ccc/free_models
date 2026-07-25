use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "model_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub timeout: i32,
    pub priority: i32,
    pub is_active: bool,
    pub context_length: i32,
    pub created_time: DateTime,
    pub last_updated: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::provider_model_map::Entity")]
    ProviderModelMap,
}

impl Related<super::provider_model_map::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProviderModelMap.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
