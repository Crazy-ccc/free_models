pub mod types;
pub mod entities;

macro_rules! impl_delete_by_id {
    ($entity:path) => {
        pub async fn delete(&self, id: i32) -> Result<bool, $crate::db::StoreError> {
            let model = <$entity>::find_by_id(id).one(&self.db).await
                .map_err($crate::db::StoreError::from)?;
            match model {
                Some(m) => {
                    m.delete(&self.db).await
                        .map_err($crate::db::StoreError::from)?;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
    };
}

pub mod impls;

use std::fmt;

#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    Database(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::NotFound(msg) => write!(f, "Not found: {}", msg),
            StoreError::Database(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<sea_orm::DbErr> for StoreError {
    fn from(e: sea_orm::DbErr) -> Self {
        StoreError::Database(e.to_string())
    }
}

use std::env;
use std::time::Duration;
use sea_orm::{ConnectOptions, ConnectionTrait, Database as SeaOrmDatabase, DatabaseBackend, DatabaseConnection, Statement};

use crate::db::impls::{
    AdminKeyStoreSeaorm, ApiKeyStoreSeaorm, ModelConfigStoreSeaorm, ProviderConfigStoreSeaorm,
    ProviderCredentialStoreSeaorm, ProviderModelMapStoreSeaorm, UsageLogStoreSeaorm,
};

pub struct Database {
    pub provider_configs: ProviderConfigStoreSeaorm,
    pub model_configs: ModelConfigStoreSeaorm,
    pub provider_credentials: ProviderCredentialStoreSeaorm,
    pub provider_model_maps: ProviderModelMapStoreSeaorm,
    pub api_keys: ApiKeyStoreSeaorm,
    pub admin_keys: AdminKeyStoreSeaorm,
    pub usage_logs: UsageLogStoreSeaorm,
}

const SCHEMA_DDL: &str = include_str!("../../migrations/001_sqlite_schema.sql");

fn split_schema_statements(ddl: &str) -> Vec<String> {
    let normalized = ddl.replace("\r\n", "\n").replace('\r', "\n");
    let mut statements = Vec::new();
    let mut buf = String::new();
    for line in normalized.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("-- >>>") {
            let stmt = buf.trim();
            if !stmt.is_empty() {
                statements.push(stmt.to_owned());
            }
            buf.clear();
            continue;
        }
        if trimmed.starts_with("--") {
            continue;
        }
        buf.push_str(line);
        buf.push('\n');
    }
    let stmt = buf.trim();
    if !stmt.is_empty() {
        statements.push(stmt.to_owned());
    }
    statements
}

async fn apply_schema(db: &DatabaseConnection) {
    for sql in split_schema_statements(SCHEMA_DDL) {
        db.execute_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("Failed to apply database schema");
    }
}

pub async fn init_db() -> DatabaseConnection {
    let url = env::var("DATABASE_URL")
    .expect("DATABASE_URL must be set");
    let max_connections = env::var("DB_MAX_CONNECTIONS")
    .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let mut opt = ConnectOptions::new(url.to_owned());
    opt.max_connections(max_connections)
        .connect_timeout(Duration::from_secs(5))
        .acquire_timeout(Duration::from_secs(60))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .sqlx_logging(false);
    let conn = SeaOrmDatabase::connect(opt).await
        .expect("Failed to connect to database");
    apply_schema(&conn).await;
    conn
}

pub async fn build_database() -> Database {
    let db = init_db().await;
    Database {
        provider_configs: ProviderConfigStoreSeaorm::new(db.clone()),
        model_configs: ModelConfigStoreSeaorm::new(db.clone()),
        provider_credentials: ProviderCredentialStoreSeaorm::new(db.clone()),
        provider_model_maps: ProviderModelMapStoreSeaorm::new(db.clone()),
        api_keys: ApiKeyStoreSeaorm::new(db.clone()),
        admin_keys: AdminKeyStoreSeaorm::new(db.clone()),
        usage_logs: UsageLogStoreSeaorm::new(db),
    }
}
