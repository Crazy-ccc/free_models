use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::db::StoreError;

use crate::db::entities::admin_key::{Column, Entity};

#[derive(Clone)]
pub struct AdminKeyStoreSeaorm {
    db: sea_orm::DatabaseConnection,
}

impl AdminKeyStoreSeaorm {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_active_by_fingerprint(&self, fingerprint: &str) -> Result<Option<crate::db::entities::admin_key::Model>, StoreError> {
        let model = Entity::find()
            .filter(Column::Fingerprint.eq(fingerprint))
            .filter(Column::IsActive.eq(true))
            .one(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(model)
    }
}
