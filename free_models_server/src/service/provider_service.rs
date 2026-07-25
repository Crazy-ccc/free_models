use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::collections::{HashMap, HashSet};

use crate::db::entities::provider_config;

pub async fn list(db: &DatabaseConnection) -> Result<Vec<provider_config::Model>, sea_orm::DbErr> {
    provider_config::Entity::find().all(db).await
}

pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Option<provider_config::Model>, sea_orm::DbErr> {
    provider_config::Entity::find_by_id(id).one(db).await
}

pub async fn create(
    db: &DatabaseConnection,
    name: &str,
    base_url: &str,
) -> Result<provider_config::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().naive_utc();
    let model = provider_config::ActiveModel {
        name: Set(name.to_string()),
        base_url: Set(base_url.to_string()),
        created_time: Set(now),
        last_updated: Set(now),
        ..Default::default()
    };
    model.insert(db).await
}

pub async fn update(
    db: &DatabaseConnection,
    id: i32,
    name: &str,
    base_url: &str,
) -> Result<provider_config::Model, sea_orm::DbErr> {
    let model = provider_config::Entity::find_by_id(id).one(db).await?;
    let model = match model {
        Some(m) => m,
        None => return Err(sea_orm::DbErr::RecordNotFound("Provider not found".into())),
    };
    let now = chrono::Utc::now().naive_utc();
    let mut active_model: provider_config::ActiveModel = model.into();
    active_model.name = Set(name.to_string());
    active_model.base_url = Set(base_url.to_string());
    active_model.last_updated = Set(now);
    active_model.update(db).await
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<bool, sea_orm::DbErr> {
    let result = provider_config::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

pub async fn list_by_ids(
    db: &DatabaseConnection,
    ids: HashSet<i32>,
) -> Result<HashMap<i32, provider_config::Model>, sea_orm::DbErr> {
    let models = provider_config::Entity::find()
        .filter(provider_config::Column::Id.is_in(ids))
        .all(db)
        .await?;
    Ok(models.into_iter().map(|m| (m.id, m)).collect())
}
