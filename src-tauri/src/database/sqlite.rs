use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Connection, Row, SqlitePool,
};
use uuid::Uuid;

use crate::{
    blueprint::{
        CanonicalType, Capability, Column, ConnectionConfig, DatabaseConfig, DatabaseProvider,
        ForeignKey, Index, Table,
    },
    error::AppError,
};

use super::{
    ApplyResult, ConnectionStatus, DatabaseAdapter, DatabaseDiagnostic, IntrospectionResult,
    MigrationPlan, SchemaOperation,
};

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

    async fn plan_schema_changes(
        &self,
        config: &DatabaseConfig,
        operations: &[SchemaOperation],
    ) -> Result<MigrationPlan, AppError> {
        require_schema(config)?;
        if operations.is_empty() {
            return Err(AppError::Validation);
        }
        validate_operations(operations)?;
        let current = self.introspect(config).await?;
        let schema_fingerprint = schema_fingerprint(config, &current)?;
        let statements = build_statements(&current, operations)?;
        let destructive = operations.iter().any(SchemaOperation::destructive);
        let namespace = Uuid::parse_str(&config.id).map_err(|_| AppError::Validation)?;
        let payload = serde_json::to_vec(&(schema_fingerprint.as_str(), operations))
            .map_err(|_| AppError::Internal)?;
        let id = Uuid::new_v5(&namespace, &payload).to_string();
        Ok(MigrationPlan {
            id,
            schema_fingerprint,
            operations: operations.to_vec(),
            sql_preview: statements.join("\n\n"),
            statements,
            destructive,
            requires_backup: true,
            confirmation_token: destructive.then(|| Uuid::new_v4().to_string()),
        })
    }

    async fn apply_schema_changes(
        &self,
        config: &DatabaseConfig,
        plan: &MigrationPlan,
        confirmation_token: Option<&str>,
    ) -> Result<ApplyResult, AppError> {
        require_schema(config)?;
        if plan.destructive && plan.confirmation_token.as_deref() != confirmation_token {
            return Err(AppError::CapabilityDenied);
        }
        let current = self.introspect(config).await?;
        if schema_fingerprint(config, &current)? != plan.schema_fingerprint {
            return Err(AppError::Conflict);
        }
        let path = sqlite_path(config)?;
        let backup = backup_sqlite(&path)?;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(false)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut connection)
            .await?;
        let mut transaction = connection.begin().await?;
        let mut applied = 0usize;
        for statement in &plan.statements {
            if statement.starts_with("PRAGMA foreign_keys") {
                continue;
            }
            if let Err(error) = sqlx::query(statement).execute(&mut *transaction).await {
                let _ = transaction.rollback().await;
                let _ = sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&mut connection)
                    .await;
                return Err(AppError::Database(error));
            }
            applied += 1;
        }
        transaction.commit().await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .await?;
        connection.close().await?;
        Ok(ApplyResult {
            plan_id: plan.id.clone(),
            backup_path: backup.to_string_lossy().into_owned(),
            statements_applied: applied,
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

fn require_schema(config: &DatabaseConfig) -> Result<(), AppError> {
    require_read(config)?;
    if !config.capabilities.contains(&Capability::Schema) {
        return Err(AppError::CapabilityDenied);
    }
    Ok(())
}

fn sqlite_path(config: &DatabaseConfig) -> Result<PathBuf, AppError> {
    match &config.connection {
        ConnectionConfig::Sqlite { path } => validate_existing_sqlite_path(Path::new(path)),
        _ => Err(AppError::Validation),
    }
}

async fn connect(config: &DatabaseConfig) -> Result<(SqlitePool, PathBuf), AppError> {
    let path = sqlite_path(config)?;
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

fn validate_operations(operations: &[SchemaOperation]) -> Result<(), AppError> {
    let mut ids = HashSet::new();
    for operation in operations {
        Uuid::parse_str(operation.operation_id()).map_err(|_| AppError::Validation)?;
        if !ids.insert(operation.operation_id()) {
            return Err(AppError::Validation);
        }
        match operation {
            SchemaOperation::AddTable { table, .. } => validate_table_definition(table)?,
            SchemaOperation::DropTable { table_name, .. } => validate_identifier(table_name)?,
            SchemaOperation::AddColumn {
                table_name, column, ..
            } => {
                validate_identifier(table_name)?;
                validate_column(column)?;
                if column.primary_key {
                    return Err(AppError::Validation);
                }
            }
            SchemaOperation::DropColumn {
                table_name,
                column_name,
                ..
            } => {
                validate_identifier(table_name)?;
                validate_identifier(column_name)?;
            }
            SchemaOperation::RenameColumn {
                table_name,
                from,
                to,
                ..
            } => {
                validate_identifier(table_name)?;
                validate_identifier(from)?;
                validate_identifier(to)?;
            }
            SchemaOperation::AddForeignKey {
                table_name,
                foreign_key,
                ..
            } => {
                validate_identifier(table_name)?;
                validate_foreign_key(foreign_key)?;
            }
            SchemaOperation::DropForeignKey { table_name, .. } => {
                validate_identifier(table_name)?;
            }
        }
    }
    Ok(())
}

fn build_statements(
    current: &IntrospectionResult,
    operations: &[SchemaOperation],
) -> Result<Vec<String>, AppError> {
    let tables: BTreeMap<_, _> = current
        .tables
        .iter()
        .map(|table| (table.name.clone(), table.clone()))
        .collect();
    let mut relation_targets: BTreeMap<String, Table> = BTreeMap::new();
    let mut relation_tables = BTreeSet::new();
    for operation in operations {
        match operation {
            SchemaOperation::AddForeignKey { table_name, .. }
            | SchemaOperation::DropForeignKey { table_name, .. } => {
                relation_tables.insert(table_name.clone());
            }
            _ => {}
        }
    }
    for table_name in &relation_tables {
        if operations.iter().any(|operation| match operation {
            SchemaOperation::AddColumn {
                table_name: value, ..
            }
            | SchemaOperation::DropColumn {
                table_name: value, ..
            }
            | SchemaOperation::RenameColumn {
                table_name: value, ..
            } => value == table_name,
            _ => false,
        }) {
            return Err(AppError::Validation);
        }
        if current.diagnostics.iter().any(|item| {
            item.object.starts_with(table_name)
                && matches!(
                    item.code.as_str(),
                    "sqlite_generated_column"
                        | "sqlite_partial_index"
                        | "sqlite_expression_index"
                        | "sqlite_without_rowid"
                        | "sqlite_strict_table"
                )
        }) {
            return Err(AppError::Validation);
        }
        relation_targets.insert(
            table_name.clone(),
            tables.get(table_name).cloned().ok_or(AppError::NotFound)?,
        );
    }
    for operation in operations {
        match operation {
            SchemaOperation::AddForeignKey {
                table_name,
                foreign_key,
                ..
            } => relation_targets
                .get_mut(table_name)
                .ok_or(AppError::NotFound)?
                .foreign_keys
                .push(foreign_key.clone()),
            SchemaOperation::DropForeignKey {
                table_name,
                foreign_key_id,
                ..
            } => {
                let table = relation_targets
                    .get_mut(table_name)
                    .ok_or(AppError::NotFound)?;
                let previous = table.foreign_keys.len();
                table.foreign_keys.retain(|item| item.id != *foreign_key_id);
                if previous == table.foreign_keys.len() {
                    return Err(AppError::NotFound);
                }
            }
            _ => {}
        }
    }

    let mut statements = vec!["PRAGMA foreign_keys = OFF;".into()];
    for operation in operations {
        match operation {
            SchemaOperation::AddTable { table, .. } => {
                if tables.contains_key(&table.name) {
                    return Err(AppError::Validation);
                }
                statements.push(format!("{};", create_table_sql(&table.name, table)?));
            }
            SchemaOperation::DropTable { table_name, .. } => {
                if !tables.contains_key(table_name) {
                    return Err(AppError::NotFound);
                }
                statements.push(format!("DROP TABLE {};", quote_identifier(table_name)));
            }
            SchemaOperation::AddColumn {
                table_name, column, ..
            } => {
                require_table_column_state(&tables, table_name, &column.name, false)?;
                statements.push(format!(
                    "ALTER TABLE {} ADD COLUMN {};",
                    quote_identifier(table_name),
                    column_definition(column, false)?
                ));
            }
            SchemaOperation::DropColumn {
                table_name,
                column_name,
                ..
            } => {
                require_table_column_state(&tables, table_name, column_name, true)?;
                statements.push(format!(
                    "ALTER TABLE {} DROP COLUMN {};",
                    quote_identifier(table_name),
                    quote_identifier(column_name)
                ));
            }
            SchemaOperation::RenameColumn {
                table_name,
                from,
                to,
                ..
            } => {
                require_table_column_state(&tables, table_name, from, true)?;
                require_table_column_state(&tables, table_name, to, false)?;
                statements.push(format!(
                    "ALTER TABLE {} RENAME COLUMN {} TO {};",
                    quote_identifier(table_name),
                    quote_identifier(from),
                    quote_identifier(to)
                ));
            }
            SchemaOperation::AddForeignKey { .. } | SchemaOperation::DropForeignKey { .. } => {}
        }
    }
    for (name, target) in relation_targets {
        statements.extend(rebuild_table_sql(
            tables.get(&name).ok_or(AppError::NotFound)?,
            &target,
        )?);
    }
    statements.push("PRAGMA foreign_keys = ON;".into());
    Ok(statements)
}

fn require_table_column_state(
    tables: &BTreeMap<String, Table>,
    table_name: &str,
    column_name: &str,
    should_exist: bool,
) -> Result<(), AppError> {
    let table = tables.get(table_name).ok_or(AppError::NotFound)?;
    let exists = table
        .columns
        .iter()
        .any(|column| column.name == column_name);
    if exists != should_exist {
        return Err(AppError::Validation);
    }
    Ok(())
}

fn rebuild_table_sql(current: &Table, target: &Table) -> Result<Vec<String>, AppError> {
    validate_table_definition(target)?;
    let temp_name = format!(
        "__emanduite_{}",
        Uuid::new_v5(&Uuid::NAMESPACE_OID, target.id.as_bytes()).simple()
    );
    let current_columns: BTreeSet<_> = current
        .columns
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    let common: Vec<_> = target
        .columns
        .iter()
        .filter(|column| current_columns.contains(column.name.as_str()))
        .map(|column| quote_identifier(&column.name))
        .collect();
    let mut result = vec![format!("{};", create_table_sql(&temp_name, target)?)];
    if !common.is_empty() {
        let columns = common.join(", ");
        result.push(format!(
            "INSERT INTO {} ({}) SELECT {} FROM {};",
            quote_identifier(&temp_name),
            columns,
            columns,
            quote_identifier(&current.name)
        ));
    }
    result.push(format!("DROP TABLE {};", quote_identifier(&current.name)));
    result.push(format!(
        "ALTER TABLE {} RENAME TO {};",
        quote_identifier(&temp_name),
        quote_identifier(&current.name)
    ));
    for index in &target.indexes {
        if index.columns.is_empty() {
            return Err(AppError::Validation);
        }
        result.push(format!(
            "CREATE {}INDEX {} ON {} ({});",
            if index.unique { "UNIQUE " } else { "" },
            quote_identifier(&index.name),
            quote_identifier(&current.name),
            index
                .columns
                .iter()
                .map(|item| quote_identifier(item))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(result)
}

fn create_table_sql(name: &str, table: &Table) -> Result<String, AppError> {
    validate_identifier(name)?;
    if table.columns.is_empty() {
        return Err(AppError::Validation);
    }
    let mut parts = table
        .columns
        .iter()
        .map(|column| column_definition(column, false))
        .collect::<Result<Vec<_>, _>>()?;
    let primary: Vec<_> = table
        .columns
        .iter()
        .filter(|column| column.primary_key)
        .map(|column| quote_identifier(&column.name))
        .collect();
    if !primary.is_empty() {
        parts.push(format!("PRIMARY KEY ({})", primary.join(", ")));
    }
    for foreign_key in &table.foreign_keys {
        validate_foreign_key(foreign_key)?;
        let mut value = format!(
            "FOREIGN KEY ({}) REFERENCES {} ({})",
            quote_identifier(&foreign_key.from_column),
            quote_identifier(&foreign_key.to_table),
            quote_identifier(&foreign_key.to_column)
        );
        if let Some(action) = &foreign_key.on_update {
            value.push_str(&format!(" ON UPDATE {}", validate_action(action)?));
        }
        if let Some(action) = &foreign_key.on_delete {
            value.push_str(&format!(" ON DELETE {}", validate_action(action)?));
        }
        parts.push(value);
    }
    Ok(format!(
        "CREATE TABLE {} (\n  {}\n)",
        quote_identifier(name),
        parts.join(",\n  ")
    ))
}

fn column_definition(column: &Column, inline_primary_key: bool) -> Result<String, AppError> {
    validate_column(column)?;
    let mut value = format!(
        "{} {}",
        quote_identifier(&column.name),
        column.native_type.trim()
    );
    if !column.nullable {
        value.push_str(" NOT NULL");
    }
    if inline_primary_key && column.primary_key {
        value.push_str(" PRIMARY KEY");
    }
    if let Some(default) = &column.default_value {
        validate_default(default)?;
        value.push_str(" DEFAULT ");
        value.push_str(default.trim());
    }
    Ok(value)
}

fn validate_table_definition(table: &Table) -> Result<(), AppError> {
    Uuid::parse_str(&table.id).map_err(|_| AppError::Validation)?;
    validate_identifier(&table.name)?;
    let mut names = HashSet::new();
    for column in &table.columns {
        validate_column(column)?;
        if !names.insert(column.name.as_str()) {
            return Err(AppError::Validation);
        }
    }
    for foreign_key in &table.foreign_keys {
        validate_foreign_key(foreign_key)?;
    }
    Ok(())
}

fn validate_column(column: &Column) -> Result<(), AppError> {
    Uuid::parse_str(&column.id).map_err(|_| AppError::Validation)?;
    validate_identifier(&column.name)?;
    let native = column.native_type.trim();
    if native.is_empty()
        || native.len() > 64
        || !native
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, ' ' | '(' | ')' | ','))
    {
        return Err(AppError::Validation);
    }
    if let Some(default) = &column.default_value {
        validate_default(default)?;
    }
    Ok(())
}

fn validate_foreign_key(value: &ForeignKey) -> Result<(), AppError> {
    Uuid::parse_str(&value.id).map_err(|_| AppError::Validation)?;
    validate_identifier(&value.from_column)?;
    validate_identifier(&value.to_table)?;
    validate_identifier(&value.to_column)?;
    if let Some(action) = &value.on_update {
        validate_action(action)?;
    }
    if let Some(action) = &value.on_delete {
        validate_action(action)?;
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty()
        || value.len() > 128
        || value.chars().any(|character| character.is_control())
    {
        return Err(AppError::Validation);
    }
    Ok(())
}

fn validate_default(value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 256
        || trimmed.contains(';')
        || trimmed.contains("--")
        || trimmed.contains("/*")
    {
        return Err(AppError::Validation);
    }
    Ok(())
}

fn validate_action(value: &str) -> Result<&'static str, AppError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "NO ACTION" => Ok("NO ACTION"),
        "RESTRICT" => Ok("RESTRICT"),
        "SET NULL" => Ok("SET NULL"),
        "SET DEFAULT" => Ok("SET DEFAULT"),
        "CASCADE" => Ok("CASCADE"),
        _ => Err(AppError::Validation),
    }
}

fn backup_sqlite(path: &Path) -> Result<PathBuf, AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    let directory = parent.join(".emanduite-backups");
    fs::create_dir_all(&directory)?;
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("database");
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let backup = directory.join(format!("{stem}-{timestamp}.sqlite"));
    fs::copy(path, &backup)?;
    fs::OpenOptions::new()
        .write(true)
        .open(&backup)?
        .sync_all()?;
    Ok(backup)
}

fn schema_fingerprint(
    config: &DatabaseConfig,
    schema: &IntrospectionResult,
) -> Result<String, AppError> {
    let namespace = Uuid::parse_str(&config.id).map_err(|_| AppError::Validation)?;
    let payload = serde_json::to_vec(schema).map_err(|_| AppError::Internal)?;
    Ok(Uuid::new_v5(&namespace, &payload).to_string())
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

    #[tokio::test]
    async fn plans_applies_and_backs_up_add_column() {
        let (_directory, config) = fixture().await;
        let operation = SchemaOperation::AddColumn {
            operation_id: Uuid::new_v4().to_string(),
            table_name: "users".into(),
            column: Column {
                id: Uuid::new_v4().to_string(),
                name: "display_name".into(),
                native_type: "TEXT".into(),
                canonical_type: CanonicalType::Text,
                nullable: true,
                primary_key: false,
                default_value: None,
            },
        };
        let plan = SqliteAdapter
            .plan_schema_changes(&config, &[operation])
            .await
            .unwrap();
        assert!(!plan.destructive);
        assert!(plan.sql_preview.contains("ADD COLUMN"));
        let applied = SqliteAdapter
            .apply_schema_changes(&config, &plan, None)
            .await
            .unwrap();
        assert!(Path::new(&applied.backup_path).is_file());
        let schema = SqliteAdapter.introspect(&config).await.unwrap();
        assert!(schema
            .tables
            .iter()
            .find(|table| table.name == "users")
            .unwrap()
            .columns
            .iter()
            .any(|column| column.name == "display_name"));
    }

    #[tokio::test]
    async fn destructive_plan_requires_exact_confirmation_token() {
        let (_directory, config) = fixture().await;
        let operation = SchemaOperation::DropColumn {
            operation_id: Uuid::new_v4().to_string(),
            table_name: "posts".into(),
            column_name: "payload".into(),
        };
        let plan = SqliteAdapter
            .plan_schema_changes(&config, &[operation])
            .await
            .unwrap();
        assert!(plan.destructive);
        assert!(matches!(
            SqliteAdapter
                .apply_schema_changes(&config, &plan, None)
                .await,
            Err(AppError::CapabilityDenied)
        ));
        SqliteAdapter
            .apply_schema_changes(&config, &plan, plan.confirmation_token.as_deref())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejects_plan_when_schema_changed_after_preview() {
        let (_directory, config) = fixture().await;
        let operation = SchemaOperation::AddColumn {
            operation_id: Uuid::new_v4().to_string(),
            table_name: "users".into(),
            column: Column {
                id: Uuid::new_v4().to_string(),
                name: "planned_name".into(),
                native_type: "TEXT".into(),
                canonical_type: CanonicalType::Text,
                nullable: true,
                primary_key: false,
                default_value: None,
            },
        };
        let plan = SqliteAdapter
            .plan_schema_changes(&config, &[operation])
            .await
            .unwrap();
        let options = SqliteConnectOptions::new()
            .filename(sqlite_path(&config).unwrap())
            .create_if_missing(false);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query("ALTER TABLE users ADD COLUMN external_change TEXT")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();

        assert!(matches!(
            SqliteAdapter
                .apply_schema_changes(&config, &plan, None)
                .await,
            Err(AppError::Conflict)
        ));
    }

    #[tokio::test]
    async fn relation_operation_rebuilds_simple_table() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relations.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE teams (id INTEGER PRIMARY KEY); CREATE TABLE members (id INTEGER PRIMARY KEY, team_id INTEGER);")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();
        let config = Blueprint::new_sqlite("Relations", path.to_string_lossy())
            .databases
            .main;
        let operation = SchemaOperation::AddForeignKey {
            operation_id: Uuid::new_v4().to_string(),
            table_name: "members".into(),
            foreign_key: ForeignKey {
                id: Uuid::new_v4().to_string(),
                from_column: "team_id".into(),
                to_table: "teams".into(),
                to_column: "id".into(),
                on_update: Some("CASCADE".into()),
                on_delete: Some("SET NULL".into()),
            },
        };
        let plan = SqliteAdapter
            .plan_schema_changes(&config, &[operation])
            .await
            .unwrap();
        assert!(plan.sql_preview.contains("FOREIGN KEY"));
        SqliteAdapter
            .apply_schema_changes(&config, &plan, None)
            .await
            .unwrap();
        let schema = SqliteAdapter.introspect(&config).await.unwrap();
        assert_eq!(
            schema
                .tables
                .iter()
                .find(|table| table.name == "members")
                .unwrap()
                .foreign_keys
                .len(),
            1
        );
    }

    #[test]
    fn rejects_sql_fragments_in_column_defaults() {
        let column = Column {
            id: Uuid::new_v4().to_string(),
            name: "unsafe_value".into(),
            native_type: "TEXT".into(),
            canonical_type: CanonicalType::Text,
            nullable: true,
            primary_key: false,
            default_value: Some("0; DROP TABLE users".into()),
        };
        assert!(matches!(
            validate_column(&column),
            Err(AppError::Validation)
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
