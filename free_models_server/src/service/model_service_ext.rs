use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};

use crate::db::entities::model_config;

pub async fn list(db: &DatabaseConnection) -> Result<Vec<model_config::Model>, sea_orm::DbErr> {
    model_config::Entity::find().all(db).await
}

pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Option<model_config::Model>, sea_orm::DbErr> {
    model_config::Entity::find_by_id(id).one(db).await
}

pub async fn create(
    db: &DatabaseConnection,
    provider_id: i32,
    name: &str,
    model_id: &str,
    timeout: i32,
    protocols: &str,
    priority: i32,
    status: String,
    context_length: i32,
) -> Result<model_config::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().naive_utc();
    let model = model_config::ActiveModel {
        provider_id: Set(provider_id),
        name: Set(name.to_string()),
        model_id: Set(model_id.to_string()),
        timeout: Set(timeout),
        protocols: Set(protocols.to_string()),
        priority: Set(priority),
        status: Set(status),
        context_length: Set(context_length),
        created_time: Set(now),
        last_updated: Set(now),
        ..Default::default()
    };
    model.insert(db).await
}

pub async fn update(
    db: &DatabaseConnection,
    id: i32,
    provider_id: i32,
    name: &str,
    model_id: &str,
    timeout: i32,
    protocols: &str,
    priority: i32,
    status: String,
    context_length: i32,
) -> Result<model_config::Model, sea_orm::DbErr> {
    let model = model_config::Entity::find_by_id(id).one(db).await?;
    let model = match model {
        Some(m) => m,
        None => return Err(sea_orm::DbErr::RecordNotFound("Model not found".into())),
    };
    let now = chrono::Utc::now().naive_utc();
    let mut active_model: model_config::ActiveModel = model.into();
    active_model.provider_id = Set(provider_id);
    active_model.name = Set(name.to_string());
    active_model.model_id = Set(model_id.to_string());
    active_model.timeout = Set(timeout);
    active_model.protocols = Set(protocols.to_string());
    active_model.priority = Set(priority);
    active_model.status = Set(status);
    active_model.context_length = Set(context_length);
    active_model.last_updated = Set(now);
    active_model.update(db).await
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<bool, sea_orm::DbErr> {
    let result = model_config::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

use sea_orm::{ColumnTrait, PaginatorTrait, QueryFilter};

/// 统计可用模型（status = 'available'）
pub async fn count_active(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    model_config::Entity::find()
        .filter(model_config::Column::Status.eq("available"))
        .count(db)
        .await
}
