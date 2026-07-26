use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter, QueryOrder, Set};

use crate::db::StoreError;

use crate::db::entities::model_config::{ActiveModel, Column, Entity};

pub struct ModelConfigStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl ModelConfigStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<crate::db::entities::model_config::Model>, StoreError> {
        let models = Entity::find().all(&self.db).await
            .map_err(StoreError::from)?;
        Ok(models)
    }

    pub async fn get(&self, id: i32) -> Result<Option<crate::db::entities::model_config::Model>, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn create(&self, name: &str, timeout: i32, priority: i32, context_length: i32, is_active: bool) -> Result<crate::db::entities::model_config::Model, StoreError> {
        let model = ActiveModel {
            name: Set(name.to_owned()),
            timeout: Set(timeout),
            priority: Set(priority),
            context_length: Set(context_length),
            is_active: Set(is_active),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn update(&self, id: i32, name: Option<&str>, timeout: Option<i32>, priority: Option<i32>, context_length: Option<i32>, is_active: Option<bool>) -> Result<crate::db::entities::model_config::Model, StoreError> {
        let model = Entity::find_by_id(id).one(&self.db).await
            .map_err(StoreError::from)?;
        let model = model.ok_or_else(|| StoreError::NotFound(format!("ModelConfig id={}", id)))?;

        let mut active: ActiveModel = model.into();
        if let Some(v) = name { active.name = Set(v.to_owned()); }
        if let Some(v) = timeout { active.timeout = Set(v); }
        if let Some(v) = priority { active.priority = Set(v); }
        if let Some(v) = context_length { active.context_length = Set(v); }
        if let Some(v) = is_active { active.is_active = Set(v); }
        let updated = active.update(&self.db).await
            .map_err(StoreError::from)?;
        Ok(updated)
    }

    impl_delete_by_id!(crate::db::entities::model_config::Entity);

    pub async fn find_by_name(&self, name: &str) -> Result<Option<crate::db::entities::model_config::Model>, StoreError> {
        let model = Entity::find()
            .filter(Column::Name.eq(name))
            .one(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn list_all_active_ordered(&self) -> Result<Vec<crate::db::entities::model_config::Model>, StoreError> {
        let models = Entity::find()
            .filter(Column::IsActive.eq(true))
            .order_by_asc(Column::Priority)
            .all(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(models)
    }
}
