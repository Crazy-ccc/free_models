use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use std::collections::HashSet;

use crate::db::entities::provider_credential;
use crate::util::encryption;

pub async fn list_by_provider(
    db: &DatabaseConnection,
    provider_id: i32,
) -> Result<Vec<provider_credential::Model>, sea_orm::DbErr> {
    provider_credential::Entity::find()
        .filter(provider_credential::Column::ProviderId.eq(provider_id))
        .all(db)
        .await
}

pub async fn get(
    db: &DatabaseConnection,
    id: i32,
) -> Result<Option<provider_credential::Model>, sea_orm::DbErr> {
    provider_credential::Entity::find_by_id(id).one(db).await
}

pub async fn create(
    db: &DatabaseConnection,
    provider_id: i32,
    name: &str,
    api_key: &str,
    account: Option<&str>,
    password: Option<&str>,
    priority: i32,
    is_active: bool,
    encryption_key: &[u8; 32],
) -> Result<provider_credential::Model, sea_orm::DbErr> {
    let encrypted_api_key = encryption::encrypt(api_key, encryption_key)
        .map_err(sea_orm::DbErr::Custom)?;
    let encrypted_password = match password {
        Some(p) => Some(
            encryption::encrypt(p, encryption_key).map_err(sea_orm::DbErr::Custom)?,
        ),
        None => None,
    };
    let now = chrono::Utc::now().naive_utc();
    let model = provider_credential::ActiveModel {
        provider_id: Set(provider_id),
        name: Set(name.to_string()),
        api_key: Set(encrypted_api_key),
        account: Set(account.map(|s| s.to_string())),
        encrypted_password: Set(encrypted_password),
        priority: Set(priority),
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
    provider_id: Option<i32>,
    name: &str,
    new_api_key: Option<&str>,
    account: Option<&str>,
    password: Option<&str>,
    priority: i32,
    is_active: bool,
    encryption_key: &[u8; 32],
) -> Result<provider_credential::Model, sea_orm::DbErr> {
    let model = provider_credential::Entity::find_by_id(id).one(db).await?;
    let model = match model {
        Some(m) => m,
        None => return Err(sea_orm::DbErr::RecordNotFound("Credential not found".into())),
    };
    let encrypted_password = match password {
        Some(p) => Some(
            encryption::encrypt(p, encryption_key).map_err(sea_orm::DbErr::Custom)?,
        ),
        None => None,
    };
    let now = chrono::Utc::now().naive_utc();
    let mut active_model: provider_credential::ActiveModel = model.into();
    if let Some(v) = provider_id {
        active_model.provider_id = Set(v);
    }
    active_model.name = Set(name.to_string());
    if let Some(key) = new_api_key {
        let encrypted = encryption::encrypt(key, encryption_key)
            .map_err(sea_orm::DbErr::Custom)?;
        active_model.api_key = Set(encrypted);
    }
    active_model.account = Set(account.map(|s| s.to_string()));
    active_model.encrypted_password = Set(encrypted_password);
    active_model.priority = Set(priority);
    active_model.is_active = Set(is_active);
    active_model.last_updated = Set(now);
    active_model.update(db).await
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<bool, sea_orm::DbErr> {
    let result = provider_credential::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected > 0)
}

pub async fn list_active_by_provider_ids(
    db: &DatabaseConnection,
    provider_ids: HashSet<i32>,
) -> Result<Vec<provider_credential::Model>, sea_orm::DbErr> {
    provider_credential::Entity::find()
        .filter(provider_credential::Column::ProviderId.is_in(provider_ids))
        .filter(provider_credential::Column::IsActive.eq(true))
        .order_by(provider_credential::Column::Priority, sea_orm::Order::Asc)
        .all(db)
        .await
}
