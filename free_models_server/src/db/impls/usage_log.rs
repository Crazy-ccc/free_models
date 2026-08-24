use std::collections::BTreeMap;

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryResult, Set, Statement, TransactionTrait, Value,
};

use crate::db::types::{UsageLogInsert, UsageLogStatItem, UsageLogStatsResponse};
use crate::db::StoreError;
use crate::db::entities::usage_log;

struct QueryContext<'a> {
    where_clause: &'a str,
    daily_where: &'a str,
    where_values: &'a [Value],
    daily_values: &'a [Value],
}

#[derive(Clone)]
pub struct UsageLogStoreSeaorm {
    db: DatabaseConnection,
}

impl UsageLogStoreSeaorm {
    pub fn new(db: DatabaseConnection) -> Self {
        UsageLogStoreSeaorm { db }
    }
}

impl UsageLogStoreSeaorm {
    pub async fn create_batch(&self, records: &[UsageLogInsert]) -> Result<(), StoreError> {
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
        usage_log::Entity::insert_many(models)
            .exec(&self.db)
            .await
            .map_err(StoreError::from)?;
        Ok(())
    }

    pub async fn query_stats(
        &self,
        group_by: &str,
        start_time: Option<&str>,
        end_time: Option<&str>,
    ) -> Result<UsageLogStatsResponse, StoreError> {
        let (where_clause, where_values) = build_where_clause(
            "request_timestamp",
            "< date(?, '+1 day')",
            start_time,
            end_time,
        )?;

        let today = chrono::Utc::now().date_naive();
        let (has_historical, has_today) = compute_date_ranges(start_time, end_time, today)?;
        let (daily_where, daily_values) = build_where_clause("stat_date", "<= ?", start_time, end_time)?;

        let ctx = QueryContext {
            where_clause: &where_clause,
            daily_where: &daily_where,
            where_values: &where_values,
            daily_values: &daily_values,
        };

        let items = self.query_merged_items(group_by, has_historical, has_today, &ctx).await?;
        let total = self.query_merged_total(group_by, has_historical, has_today, &ctx).await?
            .ok_or_else(|| StoreError::NotFound("No data".to_string()))?;

        Ok(UsageLogStatsResponse { total, items })
    }

    pub async fn archive_yesterday(&self) -> Result<u64, StoreError> {
        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        let mut txn = self.db.begin().await.map_err(StoreError::from)?;

        if Self::is_already_archived_on(&mut txn, &yesterday).await? {
            let deleted = Self::delete_raw_logs_on(&mut txn, &yesterday).await?;
            txn.commit().await.map_err(StoreError::from)?;
            return Ok(deleted);
        }

        Self::insert_daily_aggregate_on(&mut txn, &yesterday).await?;
        let deleted = Self::delete_raw_logs_on(&mut txn, &yesterday).await?;
        txn.commit().await.map_err(StoreError::from)?;
        Ok(deleted)
    }

    async fn is_already_archived_on(
        txn: &mut DatabaseTransaction,
        yesterday: &str,
    ) -> Result<bool, StoreError> {
        let backend = DatabaseBackend::Sqlite;
        let check_sql = "SELECT COUNT(*) as cnt FROM usage_log_daily WHERE stat_date = ?";
        let check_stmt = Statement::from_sql_and_values(backend, check_sql, vec![yesterday.into()]);
        let check_row = txn.query_one_raw(check_stmt).await.map_err(StoreError::from)?;
        Ok(match check_row {
            Some(row) => row.try_get_by_index::<i64>(0).unwrap_or(0) > 0,
            None => false,
        })
    }

    async fn insert_daily_aggregate_on(
        txn: &mut DatabaseTransaction,
        yesterday: &str,
    ) -> Result<(), StoreError> {
        let backend = DatabaseBackend::Sqlite;
        let insert_sql = "\
            INSERT INTO usage_log_daily \
            (stat_date, api_key_id, api_key_name, provider_config_id, provider_credential_id, provider_name, \
             model_config_id, model_name, requests, prompt_tokens, completion_tokens, \
             total_tokens, cache_hit_tokens, cache_miss_tokens, avg_duration_ms, min_duration_ms, max_duration_ms) \
            SELECT \
             DATE(request_timestamp) AS stat_date, \
             api_key_id, \
             MAX(api_key_name) AS api_key_name, \
             provider_config_id, \
             MAX(provider_credential_id) AS provider_credential_id, \
             MAX(provider_name) AS provider_name, \
             model_config_id, \
             MAX(model_name) AS model_name, \
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
            GROUP BY DATE(request_timestamp), api_key_id, provider_config_id, model_config_id";
        let insert_stmt = Statement::from_sql_and_values(backend, insert_sql, vec![yesterday.into()]);
        txn.execute_raw(insert_stmt).await.map_err(StoreError::from)?;
        Ok(())
    }

    async fn delete_raw_logs_on(
        txn: &mut DatabaseTransaction,
        yesterday: &str,
    ) -> Result<u64, StoreError> {
        let backend = DatabaseBackend::Sqlite;
        let delete_sql = "DELETE FROM usage_log WHERE DATE(request_timestamp) = ?";
        let delete_stmt = Statement::from_sql_and_values(backend, delete_sql, vec![yesterday.into()]);
        let result = txn.execute_raw(delete_stmt).await.map_err(StoreError::from)?;
        Ok(result.rows_affected())
    }
}

impl UsageLogStoreSeaorm {
    async fn query_stats_table(
        &self,
        table_name: &str,
        group_by: &str,
        where_clause: &str,
        where_values: Vec<Value>,
        for_daily: bool,
    ) -> Result<Vec<UsageLogStatItem>, StoreError> {
        let (group_select, group_by_clause, order_clause) = build_group_clauses(group_by, !for_daily)?;
        let agg_select = build_agg_select(for_daily);
        let sql = format!(
            "SELECT {}, {} FROM {} WHERE {} {} {}",
            group_select, agg_select, table_name, where_clause, group_by_clause, order_clause
        );
        let backend = self.db.get_database_backend();
        let stmt = Statement::from_sql_and_values(backend, sql, where_values);
        let rows = self.db.query_all_raw(stmt).await.map_err(StoreError::from)?;
        let mut items = Vec::new();
        for row in &rows {
            items.push(row_to_stat_item(&row, group_by)?);
        }
        Ok(items)
    }

    async fn query_stats_total_single(
        &self,
        table_name: &str,
        group_by: &str,
        where_clause: &str,
        where_values: Vec<Value>,
        for_daily: bool,
    ) -> Result<Option<UsageLogStatItem>, StoreError> {
        let agg_select = build_agg_select(for_daily);
        let sql = format!(
            "SELECT CAST(NULL AS CHAR) AS dimension_id, 'total' AS dimension_name, {} FROM {} WHERE {}",
            agg_select, table_name, where_clause
        );
        let backend = self.db.get_database_backend();
        let stmt = Statement::from_sql_and_values(backend, sql, where_values);
        match self.db.query_one_raw(stmt).await.map_err(StoreError::from)? {
            Some(row) => Ok(Some(row_to_stat_item(&row, group_by)?)),
            None => Ok(None),
        }
    }

    async fn query_merged_items(
        &self,
        group_by: &str,
        has_historical: bool,
        has_today: bool,
        ctx: &QueryContext<'_>,
    ) -> Result<Vec<UsageLogStatItem>, StoreError> {
        let mut merged: BTreeMap<(Option<String>, String), UsageLogStatItem> = BTreeMap::new();
        if has_historical {
            let items = self.query_stats_table("usage_log_daily", group_by, ctx.daily_where, ctx.daily_values.to_vec(), true).await?;
            merged = merge_stat_items(merged, items);
        }
        if has_today {
            let items = self.query_stats_table("usage_log", group_by, ctx.where_clause, ctx.where_values.to_vec(), false).await?;
            merged = merge_stat_items(merged, items);
        }
        let mut items: Vec<UsageLogStatItem> = merged.into_values().collect();
        if group_by == "day" {
            items.sort_by(|a, b| b.dimension_id.as_deref().cmp(&a.dimension_id.as_deref()));
        }
        Ok(items)
    }

    async fn query_merged_total(
        &self,
        group_by: &str,
        has_historical: bool,
        has_today: bool,
        ctx: &QueryContext<'_>,
    ) -> Result<Option<UsageLogStatItem>, StoreError> {
        let mut totals: Vec<Option<UsageLogStatItem>> = Vec::new();
        if has_historical {
            totals.push(self.query_stats_total_single("usage_log_daily", group_by, ctx.daily_where, ctx.daily_values.to_vec(), true).await?);
        }
        if has_today {
            totals.push(self.query_stats_total_single("usage_log", group_by, ctx.where_clause, ctx.where_values.to_vec(), false).await?);
        }
        Ok(totals
            .into_iter()
            .fold(None, |acc: Option<UsageLogStatItem>, opt| match (acc, opt) {
                (Some(a), Some(b)) => Some(merge_two_totals(a, b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }))
    }
}

fn build_where_clause(
    column: &str,
    upper_op: &str,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<(String, Vec<Value>), StoreError> {
    let mut conditions = Vec::new();
    let mut values = Vec::new();

    if let Some(s) = start_time {
        if chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err() {
            return Err(StoreError::Database(format!("Invalid date format: {}, expected YYYY-MM-DD", s)));
        }
        conditions.push(format!("{} >= ?", column));
        values.push(Value::String(Some(s.to_string())));
    }
    if let Some(e) = end_time {
        if chrono::NaiveDate::parse_from_str(e, "%Y-%m-%d").is_err() {
            return Err(StoreError::Database(format!("Invalid date format: {}, expected YYYY-MM-DD", e)));
        }
        conditions.push(format!("{} {}", column, upper_op));
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
         CAST(SUM(requests) AS INTEGER) AS requests, \
         CAST(SUM(prompt_tokens) AS INTEGER) AS prompt_tokens, \
         CAST(SUM(completion_tokens) AS INTEGER) AS completion_tokens, \
         CAST(SUM(total_tokens) AS INTEGER) AS total_tokens, \
         CAST(SUM(cache_hit_tokens) AS INTEGER) AS cache_hit_tokens, \
         CAST(SUM(cache_miss_tokens) AS INTEGER) AS cache_miss_tokens, \
         CAST(SUM(requests * avg_duration_ms) AS DOUBLE PRECISION) / SUM(requests) AS avg_duration_ms, \
         MIN(min_duration_ms) AS min_duration_ms, \
         MAX(max_duration_ms) AS max_duration_ms"
    } else {
        "\
         COUNT(*) AS requests, \
         CAST(SUM(prompt_tokens) AS INTEGER) AS prompt_tokens, \
         CAST(SUM(completion_tokens) AS INTEGER) AS completion_tokens, \
         CAST(SUM(total_tokens) AS INTEGER) AS total_tokens, \
         CAST(SUM(cache_hit_tokens) AS INTEGER) AS cache_hit_tokens, \
         CAST(SUM(cache_miss_tokens) AS INTEGER) AS cache_miss_tokens, \
         CAST(AVG(duration_ms) AS DOUBLE) AS avg_duration_ms, \
         MIN(duration_ms) AS min_duration_ms, \
         MAX(duration_ms) AS max_duration_ms"
    }
}

fn build_group_clauses(group_by: &str, for_usage_log: bool) -> Result<(String, String, String), StoreError> {
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
             'Credential #' || COALESCE(CAST(provider_credential_id AS CHAR), '') AS dimension_name"
                .to_string(),
            "GROUP BY provider_credential_id".to_string(),
            "ORDER BY provider_credential_id".to_string(),
        )),
        "provider_model" => Ok((
            "CAST(provider_config_id AS CHAR) || '-' || CAST(model_config_id AS CHAR) AS dimension_id, \
             provider_name || ' / ' || model_name AS dimension_name"
                .to_string(),
            "GROUP BY provider_config_id, provider_name, model_config_id, model_name".to_string(),
            "ORDER BY provider_name, model_name".to_string(),
        )),
        _ => Err(StoreError::Database(format!("Invalid group_by: {}", group_by))),
    }
}

fn merge_stat_items(mut map: BTreeMap<(Option<String>, String), UsageLogStatItem>, items: Vec<UsageLogStatItem>) -> BTreeMap<(Option<String>, String), UsageLogStatItem> {
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

fn row_to_stat_item(row: &QueryResult, dimension: &str) -> Result<UsageLogStatItem, StoreError> {
    Ok(UsageLogStatItem {
        dimension: dimension.to_string(),
        dimension_id: row.try_get_by_index::<Option<String>>(0).map_err(StoreError::from)?,
        dimension_name: row.try_get_by_index::<String>(1).map_err(StoreError::from)?,
        requests: row.try_get_by_index::<Option<i64>>(2).map_err(StoreError::from)?.unwrap_or(0),
        prompt_tokens: row.try_get_by_index::<Option<i64>>(3).map_err(StoreError::from)?.unwrap_or(0),
        completion_tokens: row.try_get_by_index::<Option<i64>>(4).map_err(StoreError::from)?.unwrap_or(0),
        total_tokens: row.try_get_by_index::<Option<i64>>(5).map_err(StoreError::from)?.unwrap_or(0),
        cache_hit_tokens: row.try_get_by_index::<Option<i64>>(6).map_err(StoreError::from)?.unwrap_or(0),
        cache_miss_tokens: row.try_get_by_index::<Option<i64>>(7).map_err(StoreError::from)?.unwrap_or(0),
        avg_duration_ms: row.try_get_by_index::<Option<f64>>(8).map_err(StoreError::from)?.unwrap_or(0.0),
        min_duration_ms: row.try_get_by_index::<Option<i32>>(9).map_err(StoreError::from)?.unwrap_or(0),
        max_duration_ms: row.try_get_by_index::<Option<i32>>(10).map_err(StoreError::from)?.unwrap_or(0),
    })
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

fn compute_date_ranges(
    start_time: Option<&str>,
    end_time: Option<&str>,
    today: chrono::NaiveDate,
) -> Result<(bool, bool), StoreError> {
    let (has_historical, has_today) = match (start_time, end_time) {
        (Some(start), Some(end)) => {
            match (
                chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d").ok(),
                chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d").ok(),
            ) {
                (Some(s), Some(e)) => {
                    let hist = s < today;
                    let today_part = e >= today - chrono::Duration::days(1);
                    (hist, today_part)
                }
                _ => (true, true),
            }
        }
        _ => (true, true),
    };
    Ok((has_historical, has_today))
}

#[cfg(test)]
mod archive_tests {
    use super::*;

    async fn setup_store() -> UsageLogStoreSeaorm {
        let db = crate::db::test_support::connect_in_memory_db().await;
        UsageLogStoreSeaorm::new(db)
    }

    fn yesterday_str() -> String {
        (chrono::Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    }

    fn log_row(
        key_id: i32,
        key_name: &str,
        provider_id: i32,
        credential_id: i32,
        model_id: i32,
        model_name: &str,
        prompt_tokens: i32,
    ) -> usage_log::ActiveModel {
        usage_log::ActiveModel {
            api_key_id: Set(Some(key_id)),
            api_key_name: Set(Some(key_name.to_string())),
            model_config_id: Set(Some(model_id)),
            provider_config_id: Set(Some(provider_id)),
            provider_credential_id: Set(Some(credential_id)),
            model_name: Set(model_name.to_string()),
            provider_name: Set("test-provider".to_string()),
            protocol: Set("openai".to_string()),
            status: Set("success".to_string()),
            error_message: Set(None),
            prompt_tokens: Set(prompt_tokens),
            completion_tokens: Set(prompt_tokens),
            total_tokens: Set(prompt_tokens * 2),
            cache_hit_tokens: Set(0),
            cache_miss_tokens: Set(prompt_tokens * 2),
            duration_ms: Set(100),
            is_stream: Set(false),
            request_timestamp: Set(chrono::Utc::now().naive_utc()),
            ..Default::default()
        }
    }

    async fn shift_all_to_yesterday(store: &UsageLogStoreSeaorm) {
        store
            .db
            .execute_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "UPDATE usage_log SET request_timestamp = ?",
                vec![format!("{} 12:00:00", yesterday_str()).into()],
            ))
            .await
            .expect("shift timestamps to yesterday");
    }

    async fn table_count(store: &UsageLogStoreSeaorm, table: &str) -> i64 {
        let row = store
            .db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                format!("SELECT COUNT(*) FROM {}", table),
            ))
            .await
            .expect("count query")
            .expect("count row");
        row.try_get_by_index::<i64>(0).expect("count value")
    }

    #[tokio::test]
    async fn archive_groups_by_unique_key_and_is_idempotent() {
        let store = setup_store().await;
        usage_log::Entity::insert_many(vec![
            log_row(1, "k1", 10, 100, 20, "mA", 10),
            log_row(1, "k1", 10, 100, 20, "mA", 4),
            log_row(1, "k1", 10, 101, 20, "mA", 6),
            log_row(1, "k1", 10, 100, 21, "mB", 5),
            log_row(2, "k2", 11, 200, 22, "mC", 7),
        ])
        .exec(&store.db)
        .await
        .expect("seed logs");
        shift_all_to_yesterday(&store).await;

        let deleted = store.archive_yesterday().await.expect("first archive");
        assert_eq!(deleted, 5);
        assert_eq!(table_count(&store, "usage_log").await, 0);
        assert_eq!(table_count(&store, "usage_log_daily").await, 3);

        let rows = store
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT stat_date, requests, total_tokens FROM usage_log_daily \
                 WHERE api_key_id = 1 AND provider_config_id = 10 AND model_config_id = 20",
                vec![],
            ))
            .await
            .expect("grouped row query");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].try_get_by_index::<String>(0).expect("stat_date"),
            yesterday_str()
        );
        assert_eq!(rows[0].try_get_by_index::<i64>(1).expect("requests"), 3);
        assert_eq!(rows[0].try_get_by_index::<i64>(2).expect("total_tokens"), 40);

        usage_log::Entity::insert_many(vec![log_row(3, "k3", 12, 300, 23, "mD", 9)])
            .exec(&store.db)
            .await
            .expect("seed late log");
        shift_all_to_yesterday(&store).await;

        let deleted = store.archive_yesterday().await.expect("second archive");
        assert_eq!(deleted, 1);
        assert_eq!(table_count(&store, "usage_log").await, 0);
        assert_eq!(table_count(&store, "usage_log_daily").await, 3);
    }

    #[tokio::test]
    async fn archive_survives_renamed_key_within_same_group() {
        let store = setup_store().await;
        usage_log::Entity::insert_many(vec![
            log_row(1, "new-name", 10, 100, 20, "mA", 8),
            log_row(1, "old-name", 10, 100, 20, "mA", 9),
        ])
        .exec(&store.db)
        .await
        .expect("seed renamed logs");
        shift_all_to_yesterday(&store).await;

        let deleted = store.archive_yesterday().await.expect("archive renamed group");
        assert_eq!(deleted, 2);

        let rows = store
            .db
            .query_all_raw(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT api_key_name, requests, prompt_tokens FROM usage_log_daily",
                vec![],
            ))
            .await
            .expect("renamed group query");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].try_get_by_index::<String>(0).expect("api_key_name"),
            "old-name"
        );
        assert_eq!(rows[0].try_get_by_index::<i64>(1).expect("requests"), 2);
        assert_eq!(
            rows[0].try_get_by_index::<i64>(2).expect("prompt_tokens"),
            17
        );
    }
}
