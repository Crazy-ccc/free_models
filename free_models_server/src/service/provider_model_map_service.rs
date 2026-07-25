use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use crate::db::entities::provider_model_map;

pub async fn list_active_by_model_id(
    db: &DatabaseConnection,
    model_id: i32,
) -> Result<Vec<provider_model_map::Model>, sea_orm::DbErr> {
    provider_model_map::Entity::find()
        .filter(provider_model_map::Column::ModelId.eq(model_id))
        .filter(provider_model_map::Column::IsActive.eq(true))
        .order_by(provider_model_map::Column::Priority, sea_orm::Order::Asc)
        .all(db)
        .await
}
