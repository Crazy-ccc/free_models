use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter};

pub async fn count_total<E>(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr>
where
    E: EntityTrait,
    <E as EntityTrait>::Model: FromQueryResult + Sized + Send + Sync,
{
    E::find().count(db).await
}

pub async fn count_active<E, C>(db: &DatabaseConnection, col: C) -> Result<u64, sea_orm::DbErr>
where
    E: EntityTrait,
    <E as EntityTrait>::Model: FromQueryResult + Sized + Send + Sync,
    C: ColumnTrait,
{
    E::find().filter(col.eq(true)).count(db).await
}
