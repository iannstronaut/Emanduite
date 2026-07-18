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
    workspace::{
        create_project, duplicate_project, open_project, save_project, ExplorerLayout,
        ProjectSession, RecentProject, WorkspaceRepository,
    },
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
        phase: "Phase 2 - Project Manager & Schema Explorer",
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

#[tauri::command]
pub fn create_project_command(
    directory: String,
    name: String,
    sqlite_path: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    CommandResponse::from_result(create_project(
        &state,
        Path::new(&directory),
        &name,
        Path::new(&sqlite_path),
    ))
}

#[tauri::command]
pub fn open_project_command(
    path: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    CommandResponse::from_result(open_project(&state, Path::new(&path)))
}

#[tauri::command]
pub fn save_project_command(
    path: String,
    blueprint: Blueprint,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    CommandResponse::from_result(save_project(&state, Path::new(&path), &blueprint))
}

#[tauri::command]
pub fn duplicate_project_command(
    source_path: String,
    target_directory: String,
    name: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    CommandResponse::from_result(duplicate_project(
        &state,
        Path::new(&source_path),
        Path::new(&target_directory),
        &name,
    ))
}

#[tauri::command]
pub fn list_recent_projects(
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<Vec<RecentProject>> {
    CommandResponse::from_result(state.recent_projects())
}

#[tauri::command]
pub fn get_active_project_path(
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<Option<String>> {
    CommandResponse::from_result(state.active_project_path())
}

#[tauri::command]
pub fn remove_recent_project(
    path: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<()> {
    CommandResponse::from_result(state.remove_reference(&path))
}

#[tauri::command]
pub fn get_explorer_layout(
    project_path: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ExplorerLayout> {
    CommandResponse::from_result(state.layout(&project_path))
}

#[tauri::command]
pub fn save_explorer_layout(
    project_path: String,
    layout: ExplorerLayout,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<()> {
    CommandResponse::from_result(state.set_layout(&project_path, layout))
}
