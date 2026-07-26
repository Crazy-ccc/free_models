use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, Set};

use crate::db::StoreError;

use crate::db::entities::provider_model_map::{ActiveModel, Column, Entity};

/// 创建 provider_model_map 记录所需的输入参数（owned，便于跨函数传递以减少参数个数）。
#[derive(Clone)]
pub struct ProviderModelMapInput {
    pub model_id: i32,
    pub provider_id: i32,
    pub provider_model_id: String,
    pub is_active: bool,
    pub priority: i32,
    pub protocols: String,
    pub status: String,
    pub timeout: Option<i32>,
    pub context_length: Option<i32>,
}

/// 更新 provider_model_map 记录所需的输入参数，所有字段均可选。
#[derive(Clone, Default)]
pub struct ProviderModelMapUpdate {
    pub provider_model_id: Option<String>,
    pub is_active: Option<bool>,
    pub priority: Option<i32>,
    pub protocols: Option<String>,
    pub status: Option<String>,
    pub timeout: Option<i32>,
    pub context_length: Option<i32>,
}

pub struct ProviderModelMapStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl ProviderModelMapStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list_filtered(&self, model_id: Option<i32>, provider_id: Option<i32>) -> Result<Vec<crate::db::entities::provider_model_map::Model>, StoreError> {
        let mut query = Entity::find();
        if let Some(mid) = model_id {
            query = query.filter(Column::ModelId.eq(mid));
        }
        if let Some(pid) = provider_id {
            query = query.filter(Column::ProviderId.eq(pid));
        }
        let models = query.all(&self.db).await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn get(&self, id: i32) -> Result<Option<crate::db::entities::provider_model_map::Model>, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn find_by_model_and_provider(&self, model_id: i32, provider_id: i32) -> Result<Option<crate::db::entities::provider_model_map::Model>, StoreError> {
        let model = Entity::find()
            .filter(Column::ModelId.eq(model_id))
            .filter(Column::ProviderId.eq(provider_id))
            .one(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn create(
        &self,
        input: &ProviderModelMapInput,
    ) -> Result<crate::db::entities::provider_model_map::Model, StoreError> {
        let now = chrono::Utc::now().naive_utc();
        let model = ActiveModel {
            model_id: Set(input.model_id),
            provider_id: Set(input.provider_id),
            provider_model_id: Set(input.provider_model_id.clone()),
            is_active: Set(input.is_active),
            priority: Set(input.priority),
            protocols: Set(input.protocols.clone()),
            status: Set(input.status.clone()),
            timeout: Set(input.timeout),
            context_length: Set(input.context_length),
            created_time: Set(now),
            last_updated: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn update(
        &self,
        id: i32,
        update: &ProviderModelMapUpdate,
    ) -> Result<crate::db::entities::provider_model_map::Model, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ProviderModelMap id={}", id)))?;

        let now = chrono::Utc::now().naive_utc();
        let mut active: ActiveModel = model.into();
        if let Some(v) = update.provider_model_id.as_deref() { active.provider_model_id = Set(v.to_owned()); }
        if let Some(v) = update.is_active { active.is_active = Set(v); }
        if let Some(v) = update.priority { active.priority = Set(v); }
        if let Some(v) = update.protocols.as_deref() { active.protocols = Set(v.to_owned()); }
        if let Some(v) = update.status.as_deref() { active.status = Set(v.to_owned()); }
        if let Some(v) = update.timeout { active.timeout = Set(Some(v)); }
        if let Some(v) = update.context_length { active.context_length = Set(Some(v)); }
        active.last_updated = Set(now);
        let updated = active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(updated)
    }

    impl_delete_by_id!(crate::db::entities::provider_model_map::Entity);
}
