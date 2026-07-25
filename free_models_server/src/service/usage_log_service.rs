use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, Set, Statement, Value};

use crate::db::entities::usage_log;

#[derive(serde::Serialize)]
pub struct UsageLogStatItem {
    pub dimension: String,
    pub dimension_id: Option<String>,
    pub dimension_name: String,
    pub requests: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub cache_hit_tokens: i64,
    pub cache_miss_tokens: i64,
    pub avg_duration_ms: f64,
    pub min_duration_ms: i32,
    pub max_duration_ms: i32,
}

#[derive(serde::Serialize)]
pub struct UsageLogStatsResponse {
    pub total: UsageLogStatItem,
    pub items: Vec<UsageLogStatItem>,
}

/// 批量插入用的数据记录，所有权完整，可跨线程发送
#[derive(Clone)]
pub struct UsageLogInsert {
    pub api_key_id: Option<i32>,
    pub api_key_name: Option<String>,
    pub model_config_id: i32,
    pub provider_config_id: i32,
    pub provider_credential_id: i32,
    pub model_name: String,
    pub provider_name: String,
    pub protocol: String,
    pub status: String,
    pub error_message: Option<String>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub cache_miss_tokens: i32,
    pub duration_ms: i32,
    pub is_stream: bool,
}

/// 批量插入多条 usage_log 记录
pub async fn create_batch(
    db: &DatabaseConnection,
    records: &[UsageLogInsert],
) -> Result<(), sea_orm::DbErr> {
    if records.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().naive_utc();
    let models: Vec<usage_log::ActiveModel> = records
        .iter()
        .map(|r| usage_log::ActiveModel {
            api_key_id: Set(r.api_key_id),
            api_key_name: Set(r.api_key_name.clone()),
            model_config_id: Set(Some(r.model_config_id)),
            provider_config_id: Set(Some(r.provider_config_id)),
            provider_credential_id: Set(Some(r.provider_credential_id)),
            model_name: Set(r.model_name.clone()),
            provider_name: Set(r.provider_name.clone()),
            protocol: Set(r.protocol.clone()),
            status: Set(r.status.clone()),
            error_message: Set(r.error_message.clone()),
            prompt_tokens: Set(r.prompt_tokens),
            completion_tokens: Set(r.completion_tokens),
            total_tokens: Set(r.total_tokens),
            cache_hit_tokens: Set(r.cache_hit_tokens),
            cache_miss_tokens: Set(r.cache_miss_tokens),
            duration_ms: Set(r.duration_ms),
            is_stream: Set(r.is_stream),
            request_timestamp: Set(now),
            ..Default::default()
        })
        .collect();
    usage_log::Entity::insert_many(models).exec(db).await?;
    Ok(())
}

fn build_where_clause(
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<(String, Vec<Value>), sea_orm::DbErr> {
    let mut conditions = Vec::new();
    let mut values = Vec::new();

    if let Some(s) = start_time {
        if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err() {
            return Err(sea_orm::DbErr::Custom(format!("Invalid date format: {}, expected YYYY-MM-DD", s)));
        }
        conditions.push("request_timestamp >= ?".to_string());
        values.push(Value::String(Some(s.to_string())));
    }
    if let Some(e) = end_time {
        if chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").is_err() {
            return Err(sea_orm::DbErr::Custom(format!("Invalid date format: {}, expected YYYY-MM-DD", e)));
        }
        conditions.push("request_timestamp < DATE_ADD(?, INTERVAL 1 DAY)".to_string());
        values.push(Value::String(Some(e.to_string())));
    }
    let where_clause = if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    };
    Ok((where_clause, values))
}

fn build_daily_where_clause(
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<(String, Vec<Value>), sea_orm::DbErr> {
    let mut conditions = Vec::new();
    let mut values = Vec::new();

    if let Some(s) = start_time {
        if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err() {
            return Err(sea_orm::DbErr::Custom(format!("Invalid date format: {}, expected YYYY-MM-DD", s)));
        }
        conditions.push("stat_date >= ?".to_string());
        values.push(Value::String(Some(s.to_string())));
    }
    if let Some(e) = end_time {
        if chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").is_err() {
            return Err(sea_orm::DbErr::Custom(format!("Invalid date format: {}, expected YYYY-MM-DD", e)));
        }
        conditions.push("stat_date <= ?".to_string());
        values.push(Value::String(Some(e.to_string())));
    }
    let where_clause = if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    };
    Ok((where_clause, values))
}

fn build_agg_select(for_daily: bool) -> &'static str {
    if for_daily {
        "\
         CAST(SUM(requests) AS SIGNED) AS requests, \
         CAST(SUM(prompt_tokens) AS SIGNED) AS prompt_tokens, \
         CAST(SUM(completion_tokens) AS SIGNED) AS completion_tokens, \
         CAST(SUM(total_tokens) AS SIGNED) AS total_tokens, \
         CAST(SUM(cache_hit_tokens) AS SIGNED) AS cache_hit_tokens, \
         CAST(SUM(cache_miss_tokens) AS SIGNED) AS cache_miss_tokens, \
         CAST(SUM(requests * avg_duration_ms) AS DOUBLE PRECISION) / SUM(requests) AS avg_duration_ms, \
         MIN(min_duration_ms) AS min_duration_ms, \
         MAX(max_duration_ms) AS max_duration_ms"
    } else {
        "\
         COUNT(*) AS requests, \
         CAST(SUM(prompt_tokens) AS SIGNED) AS prompt_tokens, \
         CAST(SUM(completion_tokens) AS SIGNED) AS completion_tokens, \
         CAST(SUM(total_tokens) AS SIGNED) AS total_tokens, \
         CAST(SUM(cache_hit_tokens) AS SIGNED) AS cache_hit_tokens, \
         CAST(SUM(cache_miss_tokens) AS SIGNED) AS cache_miss_tokens, \
         CAST(AVG(duration_ms) AS DOUBLE) AS avg_duration_ms, \
         MIN(duration_ms) AS min_duration_ms, \
         MAX(duration_ms) AS max_duration_ms"
    }
}

fn build_group_clauses(group_by: &str, for_usage_log: bool) -> Result<(String, String, String), sea_orm::DbErr> {
    match group_by {
        "provider" => Ok((
            "CAST(provider_config_id AS CHAR) AS dimension_id, provider_name AS dimension_name".to_string(),
            "GROUP BY provider_config_id, provider_name".to_string(),
            "ORDER BY provider_config_id".to_string(),
        )),
        "model" => Ok((
            "CAST(model_config_id AS CHAR) AS dimension_id, model_name AS dimension_name".to_string(),
            "GROUP BY model_config_id, model_name".to_string(),
            "ORDER BY model_config_id".to_string(),
        )),
        "api_key" => Ok((
            "CAST(api_key_id AS CHAR) AS dimension_id, api_key_name AS dimension_name".to_string(),
            "GROUP BY api_key_id, api_key_name".to_string(),
            "ORDER BY api_key_id".to_string(),
        )),
        "day" => {
            if for_usage_log {
                Ok((
                    "CAST(DATE(request_timestamp) AS CHAR) AS dimension_id, \
                     CAST(DATE(request_timestamp) AS CHAR) AS dimension_name"
                        .to_string(),
                    "GROUP BY CAST(DATE(request_timestamp) AS CHAR)".to_string(),
                    "ORDER BY CAST(DATE(request_timestamp) AS CHAR)".to_string(),
                ))
            } else {
                Ok((
                    "CAST(stat_date AS CHAR) AS dimension_id, CAST(stat_date AS CHAR) AS dimension_name".to_string(),
                    "GROUP BY stat_date".to_string(),
                    "ORDER BY stat_date".to_string(),
                ))
            }
        }
        "credential" => Ok((
            "CAST(provider_credential_id AS CHAR) AS dimension_id, \
             CONCAT('Credential #', provider_credential_id) AS dimension_name"
                .to_string(),
            "GROUP BY provider_credential_id".to_string(),
            "ORDER BY provider_credential_id".to_string(),
        )),
        "provider_model" => Ok((
            "CONCAT(provider_config_id, '-', model_config_id) AS dimension_id, \
             CONCAT(provider_name, ' / ', model_name) AS dimension_name"
                .to_string(),
            "GROUP BY provider_config_id, provider_name, model_config_id, model_name".to_string(),
            "ORDER BY provider_name, model_name".to_string(),
        )),
        _ => Err(sea_orm::DbErr::Custom(format!("Invalid group_by: {}", group_by))),
    }
}

fn merge_stat_items(mut map: std::collections::BTreeMap<(Option<String>, String), UsageLogStatItem>, items: Vec<UsageLogStatItem>) -> std::collections::BTreeMap<(Option<String>, String), UsageLogStatItem> {
    for item in items {
        let key = (item.dimension_id.clone(), item.dimension_name.clone());
        match map.get_mut(&key) {
            Some(existing) => {
                let total_requests = existing.requests + item.requests;
                existing.avg_duration_ms = if total_requests > 0 {
                    (existing.avg_duration_ms * existing.requests as f64 + item.avg_duration_ms * item.requests as f64) / total_requests as f64
                } else {
                    0.0
                };
                existing.requests = total_requests;
                existing.prompt_tokens += item.prompt_tokens;
                existing.completion_tokens += item.completion_tokens;
                existing.total_tokens += item.total_tokens;
                existing.cache_hit_tokens += item.cache_hit_tokens;
                existing.cache_miss_tokens += item.cache_miss_tokens;
                existing.max_duration_ms = existing.max_duration_ms.max(item.max_duration_ms);
                existing.min_duration_ms = existing.min_duration_ms.min(item.min_duration_ms);
            }
            None => {
                map.insert(key, item);
            }
        }
    }
    map
}

fn row_to_stat_item(row: &sea_orm::QueryResult, dimension: &str) -> Result<UsageLogStatItem, sea_orm::DbErr> {
    Ok(UsageLogStatItem {
        dimension: dimension.to_string(),
        dimension_id: row.try_get_by_index::<Option<String>>(0)?,
        dimension_name: row.try_get_by_index::<String>(1)?,
        requests: row.try_get_by_index::<i64>(2)?,
        prompt_tokens: row.try_get_by_index::<i64>(3)?,
        completion_tokens: row.try_get_by_index::<i64>(4)?,
        total_tokens: row.try_get_by_index::<i64>(5)?,
        cache_hit_tokens: row.try_get_by_index::<i64>(6)?,
        cache_miss_tokens: row.try_get_by_index::<i64>(7)?,
        avg_duration_ms: row.try_get_by_index::<f64>(8)?,
        min_duration_ms: row.try_get_by_index::<i32>(9)?,
        max_duration_ms: row.try_get_by_index::<i32>(10)?,
    })
}

async fn query_stats_table(
    db: &DatabaseConnection,
    table_name: &str,
    group_by: &str,
    where_clause: &str,
    where_values: Vec<Value>,
    for_daily: bool,
) -> Result<Vec<UsageLogStatItem>, sea_orm::DbErr> {
    let (group_select, group_by_clause, order_clause) = build_group_clauses(group_by, !for_daily)?;
    let agg_select = build_agg_select(for_daily);
    let sql = format!(
        "SELECT {}, {} FROM {} WHERE {} {} {}",
        group_select, agg_select, table_name, where_clause, group_by_clause, order_clause
    );
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, where_values);
    let rows = db.query_all_raw(stmt).await?;
    let mut items = Vec::new();
    for row in &rows {
        items.push(row_to_stat_item(&row, group_by)?);
    }
    Ok(items)
}

async fn query_stats_total_single(
    db: &DatabaseConnection,
    table_name: &str,
    group_by: &str,
    where_clause: &str,
    where_values: Vec<Value>,
    for_daily: bool,
) -> Result<Option<UsageLogStatItem>, sea_orm::DbErr> {
    let agg_select = build_agg_select(for_daily);
    let sql = format!(
        "SELECT CAST(NULL AS CHAR) AS dimension_id, 'total' AS dimension_name, {} FROM {} WHERE {}",
        agg_select, table_name, where_clause
    );
    let backend = db.get_database_backend();
    let stmt = Statement::from_sql_and_values(backend, sql, where_values);
    match db.query_one_raw(stmt).await? {
        Some(row) => Ok(Some(row_to_stat_item(&row, group_by)?)),
        None => Ok(None),
    }
}

fn merge_two_totals(a: UsageLogStatItem, b: UsageLogStatItem) -> UsageLogStatItem {
    let total_requests = a.requests + b.requests;
    let avg_duration_ms = if total_requests > 0 {
        (a.avg_duration_ms * a.requests as f64 + b.avg_duration_ms * b.requests as f64) / total_requests as f64
    } else {
        0.0
    };
    UsageLogStatItem {
        dimension: a.dimension,
        dimension_id: None,
        dimension_name: "total".to_string(),
        requests: total_requests,
        prompt_tokens: a.prompt_tokens + b.prompt_tokens,
        completion_tokens: a.completion_tokens + b.completion_tokens,
        total_tokens: a.total_tokens + b.total_tokens,
        cache_hit_tokens: a.cache_hit_tokens + b.cache_hit_tokens,
        cache_miss_tokens: a.cache_miss_tokens + b.cache_miss_tokens,
        avg_duration_ms,
        min_duration_ms: a.min_duration_ms.min(b.min_duration_ms),
        max_duration_ms: a.max_duration_ms.max(b.max_duration_ms),
    }
}

pub async fn query_stats(
    db: &DatabaseConnection,
    group_by: &str,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<UsageLogStatsResponse, sea_orm::DbErr> {
    let (where_clause, where_values) = build_where_clause(start_time, end_time)
        .map_err(|e| sea_orm::DbErr::Custom(format!("{}", e)))?;

    // credential 维度只查 usage_log（daily 表没有 credential 字段）
    if group_by == "credential" {
        let items = query_stats_table(db, "usage_log", group_by, &where_clause, where_values.clone(), false).await?;
        let total = query_stats_total_single(db, "usage_log", group_by, &where_clause, where_values, false).await?
            .ok_or_else(|| sea_orm::DbErr::Custom("No data".to_string()))?;
        return Ok(UsageLogStatsResponse { total, items });
    }

    // 非 credential 维度：根据日期范围决定查哪些表
    let today = chrono::Utc::now().date_naive();
    let (has_historical, has_today) = match (start_time, end_time) {
        (Some(start), Some(end)) => {
            match (
                chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d").ok(),
                chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d").ok(),
            ) {
                (Some(s), Some(e)) => {
                    let hist = s < today;
                    let today_part = e >= today;
                    (hist, today_part)
                }
                _ => (true, true),
            }
        }
        _ => (true, true),
    };

    let (daily_where, daily_values) = build_daily_where_clause(start_time, end_time)
        .map_err(|e| sea_orm::DbErr::Custom(format!("{}", e)))?;

    // 查询分组数据
    let mut merged: std::collections::BTreeMap<(Option<String>, String), UsageLogStatItem> = std::collections::BTreeMap::new();
    if has_historical {
        let items = query_stats_table(db, "usage_log_daily", group_by, &daily_where, daily_values.clone(), true).await?;
        merged = merge_stat_items(merged, items);
    }
    if has_today {
        let items = query_stats_table(db, "usage_log", group_by, &where_clause, where_values.clone(), false).await?;
        merged = merge_stat_items(merged, items);
    }
    let items: Vec<UsageLogStatItem> = merged.into_values().collect();

    // 查询汇总数据
    let total = match (has_historical, has_today) {
        (true, true) => {
            match (
                query_stats_total_single(db, "usage_log_daily", group_by, &daily_where, daily_values, true).await?,
                query_stats_total_single(db, "usage_log", group_by, &where_clause, where_values, false).await?,
            ) {
                (Some(a), Some(b)) => merge_two_totals(a, b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => {
                    return Err(sea_orm::DbErr::Custom("No data".to_string()));
                }
            }
        }
        (true, false) => {
            query_stats_total_single(db, "usage_log_daily", group_by, &daily_where, daily_values, true).await?
                .ok_or_else(|| sea_orm::DbErr::Custom("No data".to_string()))?
        }
        (false, true) => {
            query_stats_total_single(db, "usage_log", group_by, &where_clause, where_values, false).await?
                .ok_or_else(|| sea_orm::DbErr::Custom("No data".to_string()))?
        }
        (false, false) => {
            return Err(sea_orm::DbErr::Custom("No data".to_string()));
        }
    };

    Ok(UsageLogStatsResponse { total, items })
}

/// 归档昨日数据到 usage_log_daily，然后删除昨日原始记录
pub async fn archive_yesterday(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let backend = db.get_database_backend();

    // 先检查是否已有数据，避免重复归档
    let check_sql = "SELECT COUNT(*) as cnt FROM usage_log_daily WHERE stat_date = ?";
    let check_stmt = Statement::from_sql_and_values(backend.clone(), check_sql, vec![yesterday.clone().into()]);
    let check_row = db.query_one_raw(check_stmt).await?;
    let already_archived: bool = match check_row {
        Some(row) => row.try_get_by_index::<i64>(0).unwrap_or(0) > 0,
        None => false,
    };

    if already_archived {
        // 已归档过，删除昨日原始数据（避免重跑时重复插入）
        let delete_sql = "DELETE FROM usage_log WHERE DATE(request_timestamp) = ?";
        let delete_stmt = Statement::from_sql_and_values(backend, delete_sql, vec![yesterday.clone().into()]);
        let result = db.execute_raw(delete_stmt).await?;
        return Ok(result.rows_affected());
    }

    // 聚合插入
    let insert_sql = "\
        INSERT INTO usage_log_daily \
        (stat_date, api_key_id, api_key_name, provider_config_id, provider_name, \
         model_config_id, model_name, requests, prompt_tokens, completion_tokens, \
         total_tokens, cache_hit_tokens, cache_miss_tokens, avg_duration_ms, min_duration_ms, max_duration_ms) \
        SELECT \
         DATE(request_timestamp) AS stat_date, \
         api_key_id, api_key_name, \
         provider_config_id, provider_name, \
         model_config_id, model_name, \
         COUNT(*) AS requests, \
         SUM(prompt_tokens) AS prompt_tokens, \
         SUM(completion_tokens) AS completion_tokens, \
         SUM(total_tokens) AS total_tokens, \
         SUM(cache_hit_tokens) AS cache_hit_tokens, \
         SUM(cache_miss_tokens) AS cache_miss_tokens, \
         ROUND(AVG(duration_ms)) AS avg_duration_ms, \
         MIN(duration_ms) AS min_duration_ms, \
         MAX(duration_ms) AS max_duration_ms \
        FROM usage_log \
        WHERE DATE(request_timestamp) = ? \
        GROUP BY stat_date, api_key_id, api_key_name, provider_config_id, provider_name, \
                 model_config_id, model_name";
    let insert_stmt = Statement::from_sql_and_values(backend.clone(), insert_sql, vec![yesterday.clone().into()]);
    let _ = db.execute_raw(insert_stmt).await?;

    // 删除已归档的原始数据
    let delete_sql = "DELETE FROM usage_log WHERE DATE(request_timestamp) = ?";
    let delete_stmt = Statement::from_sql_and_values(backend, delete_sql, vec![yesterday.into()]);
    let result = db.execute_raw(delete_stmt).await?;
    Ok(result.rows_affected())
}
