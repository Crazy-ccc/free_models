use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "provider_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub base_url: String,
    pub created_time: DateTime,
    pub last_updated: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::provider_model_map::Entity")]
    ProviderModelMap,
    #[sea_orm(has_many = "super::provider_credential::Entity")]
    ProviderCredential,
}

impl Related<super::provider_model_map::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProviderModelMap.def()
    }
}

impl Related<super::provider_credential::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ProviderCredential.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}