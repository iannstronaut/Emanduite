use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{
    blueprint::{
        blueprint_json_schema, load_blueprint, save_blueprint, validate_blueprint, Blueprint,
        DatabaseConfig, ValidationDiagnostic, CURRENT_SCHEMA_VERSION,
    },
    database::{sqlite::SqliteAdapter, ConnectionStatus, DatabaseAdapter, IntrospectionResult},
    error::{AppError, CommandResponse},
    secret::{KeyringSecretStore, SecretStore},
};

pub struct SecretState(pub KeyringSecretStore);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub phase: &'static str,
    pub blueprint_schema_version: u32,
    pub database_providers: [&'static str; 1],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretReference {
    pub secret_ref: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretPresence {
    pub exists: bool,
}

#[tauri::command]
pub fn get_app_info() -> CommandResponse<AppInfo> {
    CommandResponse::from_result(Ok(AppInfo {
        name: "Emanduite",
        version: env!("CARGO_PKG_VERSION"),
        phase: "Phase 1 - Desktop Foundation",
        blueprint_schema_version: CURRENT_SCHEMA_VERSION,
        database_providers: ["sqlite"],
    }))
}

#[tauri::command]
pub fn export_blueprint_schema() -> CommandResponse<Value> {
    CommandResponse::from_result(Ok(blueprint_json_schema()))
}

#[tauri::command]
pub fn create_sqlite_blueprint(
    project_name: String,
    sqlite_path: String,
) -> CommandResponse<Blueprint> {
    let blueprint = Blueprint::new_sqlite(project_name, sqlite_path);
    CommandResponse::from_result(if validate_blueprint(&blueprint).is_empty() {
        Ok(blueprint)
    } else {
        Err(AppError::Validation)
    })
}

#[tauri::command]
pub fn validate_blueprint_command(value: Value) -> CommandResponse<Vec<ValidationDiagnostic>> {
    match serde_json::from_value::<Blueprint>(value) {
        Ok(blueprint) => CommandResponse::from_result(Ok(validate_blueprint(&blueprint))),
        Err(_) => CommandResponse::from_result(Ok(vec![ValidationDiagnostic {
            path: "$".into(),
            code: "invalid_shape".into(),
            message: "Blueprint does not match schema v1".into(),
        }])),
    }
}

#[tauri::command]
pub fn save_blueprint_command(path: String, blueprint: Blueprint) -> CommandResponse<()> {
    CommandResponse::from_result(save_blueprint(Path::new(&path), &blueprint))
}

#[tauri::command]
pub fn load_blueprint_command(path: String) -> CommandResponse<Blueprint> {
    CommandResponse::from_result(load_blueprint(Path::new(&path)))
}

#[tauri::command]
pub fn put_secret(
    project_id: String,
    key: String,
    value: String,
    state: State<'_, SecretState>,
) -> CommandResponse<SecretReference> {
    CommandResponse::from_result(
        state
            .0
            .put(&project_id, &key, &value)
            .map(|secret_ref| SecretReference { secret_ref }),
    )
}

#[tauri::command]
pub fn has_secret(
    secret_ref: String,
    state: State<'_, SecretState>,
) -> CommandResponse<SecretPresence> {
    CommandResponse::from_result(
        state
            .0
            .contains(&secret_ref)
            .map(|exists| SecretPresence { exists }),
    )
}

#[tauri::command]
pub fn delete_secret(secret_ref: String, state: State<'_, SecretState>) -> CommandResponse<()> {
    CommandResponse::from_result(state.0.delete(&secret_ref))
}

#[tauri::command]
pub async fn test_sqlite_connection(config: DatabaseConfig) -> CommandResponse<ConnectionStatus> {
    CommandResponse::from_result(SqliteAdapter.test_connection(&config).await)
}

#[tauri::command]
pub async fn introspect_sqlite(config: DatabaseConfig) -> CommandResponse<IntrospectionResult> {
    CommandResponse::from_result(SqliteAdapter.introspect(&config).await)
}
