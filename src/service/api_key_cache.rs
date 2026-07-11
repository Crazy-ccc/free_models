use std::collections::HashSet;

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::db::entities::api_key;

#[derive(Clone)]
pub struct ApiKeyCache {
    keys: HashSet<String>,
}

impl ApiKeyCache {
    /// 从数据库一次性加载所有 is_active = true 的 key_value 到内存
    pub async fn load_all(db: &DatabaseConnection) -> Self {
        let active_keys = api_key::Entity::find()
            .filter(api_key::Column::IsActive.eq(true))
            .all(db)
            .await
            .expect("Failed to load api_key cache");

        let keys: HashSet<String> = active_keys
            .into_iter()
            .map(|m| m.key_value)
            .collect();

        log::info!("Loaded {} active api keys into cache", keys.len());

        ApiKeyCache { keys }
    }

    /// 检查 key 是否存在于缓存中
    pub fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }
}
