use serde_json::Value;
use uuid::Uuid;

use crate::error::AppError;

use super::model::{Blueprint, CURRENT_SCHEMA_VERSION};

const MIGRATION_NAMESPACE: Uuid = Uuid::from_u128(0x0d6e5be5_3cb3_4ce5_a35c_3425e9284fb2);

#[derive(Debug)]
pub struct MigrationOutcome {
    pub value: Value,
    pub from_version: u32,
    pub to_version: u32,
    pub changed: bool,
}

pub fn migrate_value(mut value: Value) -> Result<MigrationOutcome, AppError> {
    let from_version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    if from_version > CURRENT_SCHEMA_VERSION {
        return Err(AppError::UnsupportedVersion);
    }
    if from_version == CURRENT_SCHEMA_VERSION {
        return Ok(MigrationOutcome {
            value,
            from_version,
            to_version: CURRENT_SCHEMA_VERSION,
            changed: false,
        });
    }

    if from_version == 0 {
        value = migrate_v0_to_v1(value)?;
    }
    Ok(MigrationOutcome {
        value,
        from_version,
        to_version: CURRENT_SCHEMA_VERSION,
        changed: true,
    })
}

fn migrate_v0_to_v1(value: Value) -> Result<Value, AppError> {
    let name = value
        .get("projectName")
        .and_then(Value::as_str)
        .unwrap_or("Migrated Project");
    let sqlite_path = value
        .get("databasePath")
        .and_then(Value::as_str)
        .unwrap_or("data.sqlite");
    let mut blueprint = Blueprint::new_sqlite(name, sqlite_path);
    blueprint.project_id = Uuid::new_v5(&MIGRATION_NAMESPACE, name.as_bytes()).to_string();
    blueprint.databases.main.id =
        Uuid::new_v5(&MIGRATION_NAMESPACE, format!("{name}:main").as_bytes()).to_string();
    blueprint.target_directory = value
        .get("targetDirectory")
        .and_then(Value::as_str)
        .map(str::to_owned);
    serde_json::to_value(blueprint).map_err(|_| AppError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_deterministic() {
        let legacy = serde_json::json!({"projectName":"Legacy","databasePath":"legacy.db"});
        let a = migrate_value(legacy.clone()).unwrap();
        let b = migrate_value(legacy).unwrap();
        assert_eq!(a.value, b.value);
        assert!(a.changed);
    }

    #[test]
    fn rejects_future_version() {
        assert!(matches!(
            migrate_value(serde_json::json!({"schemaVersion":99})),
            Err(AppError::UnsupportedVersion)
        ));
    }
}
