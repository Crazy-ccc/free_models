use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

use super::{split_schema_statements, SCHEMA_DDL};

pub(crate) async fn connect_in_memory_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");
    for stmt in split_schema_statements(SCHEMA_DDL) {
        db.execute_raw(Statement::from_string(DatabaseBackend::Sqlite, stmt))
            .await
            .expect("apply schema statement");
    }
    db
}
