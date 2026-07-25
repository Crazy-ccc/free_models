use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use std::collections::HashSet;
use crate::db::entities::{model_config, provider_credential, provider_model_map};

/// 获取所有启用的模型名称列表（去重），用于 /v1/models 接口
pub async fn get_all_model_names(
    db: &DatabaseConnection,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let models = model_config::Entity::find()
        .filter(model_config::Column::IsActive.eq(true))
        .all(db)
        .await?;

    // 从 provider_model_map 获取活跃的模型-供应商映射
    let mappings = provider_model_map::Entity::find()
        .filter(provider_model_map::Column::IsActive.eq(true))
        .all(db)
        .await?;

    // 获取有活跃凭证的供应商
    let active_provider_ids: HashSet<i32> = provider_credential::Entity::find()
        .filter(provider_credential::Column::IsActive.eq(true))
        .all(db)
        .await?
        .into_iter()
        .map(|c| c.provider_id)
        .collect();

    let active_model_ids: HashSet<i32> = mappings
        .into_iter()
        .filter(|m| active_provider_ids.contains(&m.provider_id))
        .map(|m| m.model_id)
        .collect();

    let mut names: Vec<String> = models
        .into_iter()
        .filter(|m| active_model_ids.contains(&m.id))
        .map(|m| m.name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}


