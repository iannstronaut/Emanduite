use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use uuid::Uuid;

use crate::{
    blueprint::{
        CanonicalType, Capability, Column, ConnectionConfig, DatabaseConfig, DatabaseProvider,
        ForeignKey, Index, Table,
    },
    error::AppError,
};

use super::{ConnectionStatus, DatabaseAdapter, DatabaseDiagnostic, IntrospectionResult};

pub struct SqliteAdapter;

#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn test_connection(&self, config: &DatabaseConfig) -> Result<ConnectionStatus, AppError> {
        require_read(config)?;
        let (pool, path) = connect(config).await?;
        let version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&pool)
            .await?;
        pool.close().await;
        Ok(ConnectionStatus {
            provider: "sqlite".into(),
            database_label: path.display().to_string(),
            sqlite_version: Some(version),
        })
    }

    async fn introspect(&self, config: &DatabaseConfig) -> Result<IntrospectionResult, AppError> {
        require_read(config)?;
        let (pool, _) = connect(config).await?;
        let namespace = Uuid::parse_str(&config.id).map_err(|_| AppError::Validation)?;
        let table_rows = sqlx::query("SELECT name, sql FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&pool).await?;
        let mut tables = Vec::new();
        let mut diagnostics = Vec::new();
        for row in table_rows {
            let name: String = row.try_get("name")?;
            let create_sql: Option<String> = row.try_get("sql")?;
            if let Some(sql) = create_sql {
                let upper = sql.to_ascii_uppercase();
                if upper.contains("WITHOUT ROWID") {
                    diagnostics.push(DatabaseDiagnostic {
                        code: "sqlite_without_rowid".into(),
                        object: name.clone(),
                        message: "WITHOUT ROWID is preserved by SQLite but has no canonical flag"
                            .into(),
                    });
                }
                if upper.contains(" STRICT") {
                    diagnostics.push(DatabaseDiagnostic {
                        code: "sqlite_strict_table".into(),
                        object: name.clone(),
                        message: "STRICT table mode is provider metadata in Phase 2".into(),
                    });
                }
            }
            let table_id = stable_id(&namespace, &format!("table:{name}"));
            let columns = introspect_columns(&pool, &namespace, &name, &mut diagnostics).await?;
            let foreign_keys = introspect_foreign_keys(&pool, &namespace, &name).await?;
            let indexes = introspect_indexes(&pool, &namespace, &name, &mut diagnostics).await?;
            tables.push(Table {
                id: table_id,
                name,
                columns,
                foreign_keys,
                indexes,
            });
        }
        let view_rows =
            sqlx::query("SELECT name FROM sqlite_schema WHERE type = 'view' ORDER BY name")
                .fetch_all(&pool)
                .await?;
        for row in view_rows {
            let name: String = row.try_get("name")?;
            diagnostics.push(DatabaseDiagnostic {
                code: "sqlite_view_read_only".into(),
                object: name,
                message: "Views are detected but are not canonical editable tables".into(),
            });
        }
        pool.close().await;
        Ok(IntrospectionResult {
            tables,
            diagnostics,
        })
    }
}

fn require_read(config: &DatabaseConfig) -> Result<(), AppError> {
    if config.provider != DatabaseProvider::Sqlite {
        return Err(AppError::Validation);
    }
    if !config.capabilities.contains(&Capability::Read) {
        return Err(AppError::CapabilityDenied);
    }
    Ok(())
}

async fn connect(config: &DatabaseConfig) -> Result<(SqlitePool, PathBuf), AppError> {
    let path = match &config.connection {
        ConnectionConfig::Sqlite { path } => validate_existing_sqlite_path(Path::new(path))?,
        _ => return Err(AppError::Validation),
    };
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    Ok((pool, path))
}

pub fn validate_existing_sqlite_path(path: &Path) -> Result<PathBuf, AppError> {
    if !path.is_absolute()
        || path.as_os_str().is_empty()
        || path.to_string_lossy().contains('\0')
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(AppError::InvalidPath);
    }
    let canonical = path.canonicalize().map_err(|_| AppError::InvalidPath)?;
    if !canonical.is_file() {
        return Err(AppError::InvalidPath);
    }
    Ok(canonical)
}

async fn introspect_columns(
    pool: &SqlitePool,
    namespace: &Uuid,
    table: &str,
    diagnostics: &mut Vec<DatabaseDiagnostic>,
) -> Result<Vec<Column>, AppError> {
    let query = format!("PRAGMA table_xinfo({})", quote_identifier(table));
    let rows = sqlx::query(&query).fetch_all(pool).await?;
    let mut columns = Vec::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        let native_type: String = row.try_get("type")?;
        let canonical_type = canonical_type(&native_type);
        if canonical_type == CanonicalType::Unknown {
            diagnostics.push(DatabaseDiagnostic {
                code: "unknown_sqlite_type".into(),
                object: format!("{table}.{name}"),
                message:
                    "SQLite declared type could not be mapped and is preserved as provider metadata"
                        .into(),
            });
        }
        let not_null: i64 = row.try_get("notnull")?;
        let primary_key: i64 = row.try_get("pk")?;
        let hidden: i64 = row.try_get("hidden")?;
        if hidden != 0 {
            diagnostics.push(DatabaseDiagnostic {
                code: "sqlite_generated_column".into(),
                object: format!("{table}.{name}"),
                message: "Generated/hidden column is read-only provider metadata".into(),
            });
        }
        columns.push(Column {
            id: stable_id(namespace, &format!("table:{table}:column:{name}")),
            name,
            native_type,
            canonical_type,
            nullable: not_null == 0 && primary_key == 0,
            primary_key: primary_key > 0,
            default_value: row.try_get("dflt_value")?,
        });
    }
    Ok(columns)
}

async fn introspect_foreign_keys(
    pool: &SqlitePool,
    namespace: &Uuid,
    table: &str,
) -> Result<Vec<ForeignKey>, AppError> {
    let rows = sqlx::query(&format!(
        "PRAGMA foreign_key_list({})",
        quote_identifier(table)
    ))
    .fetch_all(pool)
    .await?;
    let mut values = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let seq: i64 = row.try_get("seq")?;
        let from: String = row.try_get("from")?;
        let target_table: String = row.try_get("table")?;
        let target_column: String = row.try_get("to")?;
        values.push(ForeignKey {
            id: stable_id(namespace, &format!("table:{table}:fk:{id}:{seq}")),
            from_column: from,
            to_table: target_table,
            to_column: target_column,
            on_update: row.try_get("on_update")?,
            on_delete: row.try_get("on_delete")?,
        });
    }
    Ok(values)
}

async fn introspect_indexes(
    pool: &SqlitePool,
    namespace: &Uuid,
    table: &str,
    diagnostics: &mut Vec<DatabaseDiagnostic>,
) -> Result<Vec<Index>, AppError> {
    let rows = sqlx::query(&format!("PRAGMA index_list({})", quote_identifier(table)))
        .fetch_all(pool)
        .await?;
    let mut values = Vec::new();
    for row in rows {
        let name: String = row.try_get("name")?;
        let unique: i64 = row.try_get("unique")?;
        let partial: i64 = row.try_get("partial")?;
        if partial != 0 {
            diagnostics.push(DatabaseDiagnostic {
                code: "sqlite_partial_index".into(),
                object: format!("{table}.{name}"),
                message: "Partial index predicate is not represented in the canonical model".into(),
            });
        }
        let column_rows = sqlx::query(&format!("PRAGMA index_info({})", quote_identifier(&name)))
            .fetch_all(pool)
            .await?;
        let mut columns = Vec::new();
        for column in column_rows {
            let column_name: Option<String> = column.try_get("name")?;
            if let Some(column_name) = column_name {
                columns.push(column_name);
            } else {
                diagnostics.push(DatabaseDiagnostic {
                    code: "sqlite_expression_index".into(),
                    object: format!("{table}.{name}"),
                    message: "Expression index term is not represented as a canonical column"
                        .into(),
                });
            }
        }
        values.push(Index {
            id: stable_id(namespace, &format!("table:{table}:index:{name}")),
            name,
            unique: unique != 0,
            columns,
        });
    }
    Ok(values)
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn stable_id(namespace: &Uuid, value: &str) -> String {
    Uuid::new_v5(namespace, value.as_bytes()).to_string()
}

fn canonical_type(native: &str) -> CanonicalType {
    let value = native.trim().to_ascii_uppercase();
    if value.is_empty() || value.contains("BLOB") {
        CanonicalType::Bytes
    } else if value.contains("INT") {
        CanonicalType::Integer
    } else if value.contains("CHAR") || value.contains("CLOB") || value.contains("TEXT") {
        CanonicalType::Text
    } else if value.contains("BOOL") {
        CanonicalType::Boolean
    } else if value.contains("DATE") && value.contains("TIME") {
        CanonicalType::DateTime
    } else if value == "DATE" {
        CanonicalType::Date
    } else if value.contains("JSON") {
        CanonicalType::Json
    } else if value.contains("REAL") || value.contains("FLOA") || value.contains("DOUB") {
        CanonicalType::Real
    } else if value.contains("NUM") || value.contains("DEC") {
        CanonicalType::Decimal
    } else {
        CanonicalType::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::Blueprint;
    use sqlx::Connection;

    async fn fixture() -> (tempfile::TempDir, DatabaseConfig) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE, profile JSON, created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP); CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL, title TEXT DEFAULT 'draft', rating DECIMAL(5,2), payload BLOB, slug TEXT GENERATED ALWAYS AS (lower(title)) STORED, FOREIGN KEY(user_id) REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE); CREATE INDEX posts_title_idx ON posts(title, rating); CREATE INDEX posts_positive_rating_idx ON posts(rating) WHERE rating > 0; CREATE VIEW post_titles AS SELECT id, title FROM posts;").execute(&mut connection).await.unwrap();
        connection.close().await.unwrap();
        let blueprint = Blueprint::new_sqlite("Fixture", path.to_string_lossy());
        (directory, blueprint.databases.main)
    }

    #[tokio::test]
    async fn tests_connection_and_introspects_schema() {
        let (_directory, config) = fixture().await;
        let status = SqliteAdapter.test_connection(&config).await.unwrap();
        assert_eq!(status.provider, "sqlite");
        let result = SqliteAdapter.introspect(&config).await.unwrap();
        assert_eq!(result.tables.len(), 2);
        let posts = result
            .tables
            .iter()
            .find(|table| table.name == "posts")
            .unwrap();
        assert_eq!(posts.foreign_keys.len(), 1);
        assert!(posts
            .indexes
            .iter()
            .any(|index| index.name == "posts_title_idx"));
        assert_eq!(
            posts
                .columns
                .iter()
                .find(|column| column.name == "rating")
                .unwrap()
                .canonical_type,
            CanonicalType::Decimal
        );
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.code == "sqlite_generated_column"));
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.code == "sqlite_partial_index"));
        assert!(result
            .diagnostics
            .iter()
            .any(|item| item.code == "sqlite_view_read_only"));
        let repeated = SqliteAdapter.introspect(&config).await.unwrap();
        assert_eq!(result.tables, repeated.tables);
    }

    #[tokio::test]
    async fn denies_read_without_capability() {
        let (_directory, mut config) = fixture().await;
        config.capabilities.remove(&Capability::Read);
        assert!(matches!(
            SqliteAdapter.test_connection(&config).await,
            Err(AppError::CapabilityDenied)
        ));
    }

    #[test]
    fn rejects_relative_sqlite_path() {
        assert!(matches!(
            validate_existing_sqlite_path(Path::new("fixture.sqlite")),
            Err(AppError::InvalidPath)
        ));
    }
}
