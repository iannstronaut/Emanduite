use std::collections::HashSet;

use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::model::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValidationDiagnostic {
    pub path: String,
    pub code: String,
    pub message: String,
}

fn diagnostic(path: impl Into<String>, code: &str, message: &str) -> ValidationDiagnostic {
    ValidationDiagnostic {
        path: path.into(),
        code: code.into(),
        message: message.into(),
    }
}

pub fn validate_blueprint(blueprint: &Blueprint) -> Vec<ValidationDiagnostic> {
    let mut result = Vec::new();
    if blueprint.schema_version != CURRENT_SCHEMA_VERSION {
        result.push(diagnostic(
            "schemaVersion",
            "unsupported_version",
            "Blueprint schema version is not supported",
        ));
    }
    validate_uuid(&blueprint.project_id, "projectId", &mut result);
    if blueprint.project_name.trim().is_empty() {
        result.push(diagnostic(
            "projectName",
            "required",
            "Project name is required",
        ));
    }

    let mut ids = HashSet::new();
    check_unique_id(&blueprint.project_id, "projectId", &mut ids, &mut result);
    validate_database(
        &blueprint.databases.main,
        "databases.main",
        false,
        &mut ids,
        &mut result,
    );
    for (index, side) in blueprint.databases.sides.iter().enumerate() {
        validate_database(
            side,
            &format!("databases.sides[{index}]"),
            true,
            &mut ids,
            &mut result,
        );
    }
    for (key, entity) in &blueprint.entities {
        let path = format!("entities.{key}");
        check_unique_id(&entity.id, &format!("{path}.id"), &mut ids, &mut result);
        for (field_key, field) in &entity.fields {
            check_unique_id(
                &field.id,
                &format!("{path}.fields.{field_key}.id"),
                &mut ids,
                &mut result,
            );
        }
    }
    for (key, resource) in &blueprint.resources {
        check_unique_id(
            &resource.id,
            &format!("resources.{key}.id"),
            &mut ids,
            &mut result,
        );
        if resource.key.trim().is_empty() {
            result.push(diagnostic(
                format!("resources.{key}.key"),
                "required",
                "Resource key is required",
            ));
        }
    }
    result
}

fn validate_database(
    db: &DatabaseConfig,
    path: &str,
    side: bool,
    ids: &mut HashSet<String>,
    result: &mut Vec<ValidationDiagnostic>,
) {
    check_unique_id(&db.id, &format!("{path}.id"), ids, result);
    if db.name.trim().is_empty() {
        result.push(diagnostic(
            format!("{path}.name"),
            "required",
            "Database name is required",
        ));
    }
    if !db.capabilities.contains(&Capability::Read) {
        result.push(diagnostic(
            format!("{path}.capabilities"),
            "read_required",
            "Every database must allow read access",
        ));
    }
    if side && db.capabilities.contains(&Capability::Schema) {
        result.push(diagnostic(
            format!("{path}.capabilities"),
            "side_schema_denied",
            "Side database schema is always read-only",
        ));
    }
    match (&db.provider, &db.connection) {
        (DatabaseProvider::Sqlite, ConnectionConfig::Sqlite { path: sqlite_path })
            if sqlite_path.trim().is_empty() =>
        {
            result.push(diagnostic(
                format!("{path}.connection.path"),
                "required",
                "SQLite path is required",
            ))
        }
        (DatabaseProvider::Sqlite, ConnectionConfig::Sqlite { .. })
        | (
            DatabaseProvider::Postgresql | DatabaseProvider::Mysql,
            ConnectionConfig::Server { .. },
        ) => {}
        _ => result.push(diagnostic(
            format!("{path}.connection"),
            "provider_mismatch",
            "Connection kind does not match provider",
        )),
    }
    for (table_index, table) in db.tables.iter().enumerate() {
        let table_path = format!("{path}.tables[{table_index}]");
        check_unique_id(&table.id, &format!("{table_path}.id"), ids, result);
        for (column_index, column) in table.columns.iter().enumerate() {
            check_unique_id(
                &column.id,
                &format!("{table_path}.columns[{column_index}].id"),
                ids,
                result,
            );
        }
        for (fk_index, fk) in table.foreign_keys.iter().enumerate() {
            check_unique_id(
                &fk.id,
                &format!("{table_path}.foreignKeys[{fk_index}].id"),
                ids,
                result,
            );
        }
        for (index_index, index) in table.indexes.iter().enumerate() {
            check_unique_id(
                &index.id,
                &format!("{table_path}.indexes[{index_index}].id"),
                ids,
                result,
            );
        }
    }
}

fn validate_uuid(value: &str, path: &str, result: &mut Vec<ValidationDiagnostic>) {
    if Uuid::parse_str(value).is_err() {
        result.push(diagnostic(
            path,
            "invalid_uuid",
            "A stable UUID is required",
        ));
    }
}

fn check_unique_id(
    value: &str,
    path: &str,
    ids: &mut HashSet<String>,
    result: &mut Vec<ValidationDiagnostic>,
) {
    validate_uuid(value, path, result);
    if !ids.insert(value.to_owned()) {
        result.push(diagnostic(
            path,
            "duplicate_id",
            "Stable IDs must be globally unique within a blueprint",
        ));
    }
}

pub fn blueprint_json_schema() -> Value {
    serde_json::to_value(schema_for!(Blueprint)).expect("Blueprint schema is serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_new_sqlite_blueprint() {
        let blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        assert_eq!(validate_blueprint(&blueprint), Vec::new());
    }

    #[test]
    fn rejects_duplicate_stable_ids() {
        let mut blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        blueprint.databases.main.id = blueprint.project_id.clone();
        assert!(validate_blueprint(&blueprint)
            .iter()
            .any(|item| item.code == "duplicate_id"));
    }

    #[test]
    fn exports_json_schema() {
        let schema = blueprint_json_schema();
        assert!(schema.get("definitions").is_some() || schema.get("$defs").is_some());
    }
}
