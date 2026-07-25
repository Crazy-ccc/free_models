use std::collections::HashSet;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use sea_orm::{ColumnTrait, QueryFilter};

use crate::db::entities::api_key;

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

pub async fn list(db: &DatabaseConnection) -> Result<Vec<api_key::Model>, sea_orm::DbErr> {
    api_key::Entity::find().all(db).await
}

pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Option<api_key::Model>, sea_orm::DbErr> {
    api_key::Entity::find_by_id(id).one(db).await
}

pub async fn create(
    db: &DatabaseConnection,
    key_value: Option<String>,
    name: &str,
    is_active: bool,
) -> Result<api_key::Model, sea_orm::DbErr> {
    let now = chrono::Utc::now().naive_utc();
    let kv = key_value.unwrap_or_else(generate_key_value);
    let model = api_key::ActiveModel {
        key_value: Set(kv),
        name: Set(name.to_string()),
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
    key_value: Option<&str>,
    name: Option<&str>,
    is_active: Option<bool>,
) -> Result<api_key::Model, sea_orm::DbErr> {
    let model = api_key::Entity::find_by_id(id).one(db).await?;
    let model = match model {
        Some(m) => m,
        None => return Err(sea_orm::DbErr::RecordNotFound("API key not found".into())),
    };
    let now = chrono::Utc::now().naive_utc();
    let mut active_model: api_key::ActiveModel = model.into();
    if let Some(kv) = key_value {
        active_model.key_value = Set(kv.to_string());
    }
    if let Some(n) = name {
        active_model.name = Set(n.to_string());
    }
    if let Some(active) = is_active {
        active_model.is_active = Set(active);
    }
    active_model.last_updated = Set(now);
    active_model.update(db).await
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<bool, sea_orm::DbErr> {
    let result = api_key::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

pub async fn get_by_key_value(
    db: &DatabaseConnection,
    key_value: &str,
) -> Result<Option<api_key::Model>, sea_orm::DbErr> {
    api_key::Entity::find()
        .filter(api_key::Column::KeyValue.eq(key_value))
        .one(db)
        .await
}

pub async fn load_active_keys(db: &DatabaseConnection) -> HashSet<String> {
    let active_keys = api_key::Entity::find()
        .filter(api_key::Column::IsActive.eq(true))
        .all(db)
        .await
        .expect("Failed to load active api keys");

    active_keys.into_iter().map(|m| m.key_value).collect()
}
