use std::collections::HashSet;

use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};

use crate::db::StoreError;

use crate::db::entities::provider_credential::{ActiveModel, Column, Entity};

/// 创建 provider credential 所需的输入参数（owned，便于跨函数传递以减少参数个数）。
#[derive(Clone)]
pub struct CredentialInput {
    pub provider_id: i32,
    pub name: String,
    pub api_key: String,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: i32,
    pub is_active: bool,
}

/// 更新 provider credential 所需的输入参数，所有字段均可选。
#[derive(Clone, Default)]
pub struct CredentialUpdate {
    pub provider_id: Option<i32>,
    pub name: Option<String>,
    pub new_api_key: Option<String>,
    pub account: Option<String>,
    pub password: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Clone)]
pub struct ProviderCredentialStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl ProviderCredentialStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<crate::db::entities::provider_credential::Model>, StoreError> {
        let models = Entity::find().all(&self.db).await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn get(&self, id: i32) -> Result<Option<crate::db::entities::provider_credential::Model>, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn list_by_provider(&self, provider_id: i32) -> Result<Vec<crate::db::entities::provider_credential::Model>, StoreError> {
        let models = Entity::find()
            .filter(Column::ProviderId.eq(provider_id))
            .all(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn list_active_by_provider_ids(&self, provider_ids: &HashSet<i32>) -> Result<Vec<crate::db::entities::provider_credential::Model>, StoreError> {
        let models = Entity::find()
            .filter(Column::ProviderId.is_in(provider_ids.iter().copied()))
            .filter(Column::IsActive.eq(true))
            .all(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn create(&self, input: &CredentialInput) -> Result<crate::db::entities::provider_credential::Model, StoreError> {
        let model = ActiveModel {
            provider_id: Set(input.provider_id),
            name: Set(input.name.clone()),
            api_key: Set(input.api_key.clone()),
            account: Set(input.account.clone()),
            encrypted_password: Set(input.password.clone()),
            priority: Set(input.priority),
            is_active: Set(input.is_active),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn update(&self, id: i32, update: &CredentialUpdate) -> Result<crate::db::entities::provider_credential::Model, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ProviderCredential id={}", id)))?;

        let mut active: ActiveModel = model.into();
        if let Some(v) = update.provider_id { active.provider_id = Set(v); }
        if let Some(v) = &update.name { active.name = Set(v.clone()); }
        if let Some(v) = &update.new_api_key {
            active.api_key = Set(v.clone());
            active.quota_exhausted = Set(false);
        }
        if let Some(v) = &update.account { active.account = Set(Some(v.clone())); }
        if let Some(v) = &update.password { active.encrypted_password = Set(Some(v.clone())); }
        if let Some(v) = update.priority { active.priority = Set(v); }
        if let Some(v) = update.is_active { active.is_active = Set(v); }
        let updated = active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(updated)
    }

    pub async fn mark_quota_exhausted(&self, id: i32) -> Result<(), StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ProviderCredential id={}", id)))?;

        let mut active: ActiveModel = model.into();
        active.quota_exhausted = Set(true);
        active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(())
    }

    pub async fn clear_quota_exhausted(&self, id: i32) -> Result<(), StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ProviderCredential id={}", id)))?;

        let mut active: ActiveModel = model.into();
        active.quota_exhausted = Set(false);
        active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(())
    }

    impl_delete_by_id!(crate::db::entities::provider_credential::Entity);
}
