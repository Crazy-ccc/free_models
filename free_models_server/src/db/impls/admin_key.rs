use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use crate::db::StoreError;

use crate::db::entities::admin_key::{ActiveModel, Column, Entity};

#[derive(Clone)]
pub struct AdminKeyStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl AdminKeyStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_active_by_fingerprint(&self, fingerprint: &str) -> Result<Option<crate::db::entities::admin_key::Model>, StoreError> {
        let model = self.find_by_fingerprint(fingerprint).await?;
        Ok(model.filter(|m| m.is_active))
    }

    pub async fn find_by_fingerprint(&self, fingerprint: &str) -> Result<Option<crate::db::entities::admin_key::Model>, StoreError> {
        let model = Entity::find()
            .filter(Column::Fingerprint.eq(fingerprint))
            .one(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn create(&self, name: &str, public_key: &str, fingerprint: &str) -> Result<crate::db::entities::admin_key::Model, StoreError> {
        let model = ActiveModel {
            name: Set(name.to_owned()),
            public_key: Set(public_key.to_owned()),
            fingerprint: Set(Some(fingerprint.to_owned())),
            is_active: Set(true),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(StoreError::from)?;
        Ok(model)
    }

    pub async fn count_all(&self) -> Result<i64, StoreError> {
        let count = Entity::find().count(&self.db).await
            .map_err(StoreError::from)? as i64;
        Ok(count)
    }
}
