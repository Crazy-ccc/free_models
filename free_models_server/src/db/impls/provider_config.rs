use std::collections::{HashMap, HashSet};

use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};

use crate::db::StoreError;

use crate::db::entities::provider_config::{ActiveModel, Column, Entity};

pub struct ProviderConfigStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl ProviderConfigStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<crate::db::entities::provider_config::Model>, StoreError> {
        let models = Entity::find().all(&self.db).await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn get(&self, id: i32) -> Result<Option<crate::db::entities::provider_config::Model>, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn create(&self, name: &str, base_url: &str) -> Result<crate::db::entities::provider_config::Model, StoreError> {
        let model = ActiveModel {
            name: Set(name.to_owned()),
            base_url: Set(base_url.to_owned()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn update(&self, id: i32, name: &str, base_url: &str) -> Result<crate::db::entities::provider_config::Model, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ProviderConfig id={}" , id)))?;

        let mut active: ActiveModel = model.into();
        active.name = Set(name.to_owned());
        active.base_url = Set(base_url.to_owned());
        let updated = active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(updated)
    }

    impl_delete_by_id!(crate::db::entities::provider_config::Entity);

    pub async fn list_by_ids(&self, ids: &HashSet<i32>) -> Result<HashMap<i32, crate::db::entities::provider_config::Model>, StoreError> {
        let models = Entity::find()
            .filter(Column::Id.is_in(ids.iter().copied()))
            .all(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(models.into_iter().map(|m| (m.id, m)).collect())
    }
}
