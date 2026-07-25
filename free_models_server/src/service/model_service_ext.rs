use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set};

use crate::db::entities::model_config;

pub async fn list(db: &DatabaseConnection) -> Result<Vec<model_config::Model>, sea_orm::DbErr> {
    model_config::Entity::find().all(db).await
}

pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Option<model_config::Model>, sea_orm::DbErr> {
    model_config::Entity::find_by_id(id).one(db).await
}

pub async fn create(
    db: &DatabaseConnection,
    name: &str,
    timeout: i32,
    priority: i32,
    context_length: i32,
    is_active: bool,
) -> Result<model_config::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().naive_utc();
    let model = model_config::ActiveModel {
        name: Set(name.to_string()),
        timeout: Set(timeout),
        priority: Set(priority),
        context_length: Set(context_length),
        is_active: Set(is_active),
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
    timeout: i32,
    priority: i32,
    context_length: i32,
    is_active: bool,
) -> Result<model_config::Model, sea_orm::DbErr> {
    let model = model_config::Entity::find_by_id(id).one(db).await?;
    let model = match model {
        Some(m) => m,
        None => return Err(sea_orm::DbErr::RecordNotFound("Model not found".into())),
    };
    let now = chrono::Utc::now().naive_utc();
    let mut active_model: model_config::ActiveModel = model.into();
    active_model.name = Set(name.to_string());
    active_model.timeout = Set(timeout);
    active_model.priority = Set(priority);
    active_model.context_length = Set(context_length);
    active_model.is_active = Set(is_active);
    active_model.last_updated = Set(now);
    active_model.update(db).await
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<bool, sea_orm::DbErr> {
    let result = model_config::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

/// 返回所有活跃模型，按 priority 升序
pub async fn list_all_active_ordered(
    db: &DatabaseConnection,
) -> Result<Vec<model_config::Model>, sea_orm::DbErr> {
    model_config::Entity::find()
        .filter(model_config::Column::IsActive.eq(true))
        .order_by_asc(model_config::Column::Priority)
        .all(db)
        .await
}

/// 统计可用模型（is_active = true）
pub async fn count_active(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    model_config::Entity::find()
        .filter(model_config::Column::IsActive.eq(true))
        .count(db)
        .await
}
