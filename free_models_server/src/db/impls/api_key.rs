use std::collections::HashSet;

use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};

use crate::db::StoreError;

use crate::db::entities::api_key::{ActiveModel, Column, Entity};

const KEY_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

fn generate_key_value() -> String {
    let key: String = (0..64)
        .map(|_| {
            let idx = rand::random_range(0..KEY_CHARSET.len());
            KEY_CHARSET[idx] as char
        })
        .collect();
    format!("fm-{}", key)
}

pub struct ApiKeyStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl ApiKeyStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<crate::db::entities::api_key::Model>, StoreError> {
        let models = Entity::find().all(&self.db).await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn get(&self, id: i32) -> Result<Option<crate::db::entities::api_key::Model>, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn find_by_key_value(&self, key_value: &str) -> Result<Option<crate::db::entities::api_key::Model>, StoreError> {
        let model = Entity::find()
            .filter(Column::KeyValue.eq(key_value))
            .one(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn create(&self, key_value: Option<String>, name: &str, is_active: bool) -> Result<crate::db::entities::api_key::Model, StoreError> {
        let kv = key_value.unwrap_or_else(generate_key_value);
        let model = ActiveModel {
            key_value: Set(kv),
            name: Set(name.to_owned()),
            is_active: Set(is_active),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn update(&self, id: i32, key_value: Option<&str>, name: Option<&str>, is_active: Option<bool>) -> Result<crate::db::entities::api_key::Model, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ApiKey id={}", id)))?;

        let mut active: ActiveModel = model.into();
        if let Some(v) = key_value { active.key_value = Set(v.to_owned()); }
        if let Some(v) = name { active.name = Set(v.to_owned()); }
        if let Some(v) = is_active { active.is_active = Set(v); }
        let updated = active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(updated)
    }

    impl_delete_by_id!(crate::db::entities::api_key::Entity);

    pub async fn load_active_keys(&self) -> Result<HashSet<String>, StoreError> {
        let models = Entity::find()
            .filter(Column::IsActive.eq(true))
            .all(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(models.into_iter().map(|m| m.key_value).collect())
    }
}
