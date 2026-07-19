use std::{collections::HashMap, path::Path, sync::Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, State};

use crate::{
    blueprint::{
        blueprint_json_schema, load_blueprint, recover_blueprint, save_blueprint,
        validate_blueprint, Blueprint, DatabaseConfig, ValidationDiagnostic,
        CURRENT_SCHEMA_VERSION,
    },
    database::{
        sqlite::SqliteAdapter, ApplyResult, ConnectionStatus, DatabaseAdapter, IntrospectionResult,
        MigrationPlan, SchemaOperation,
    },
    error::{AppError, CommandResponse},
    extension::{load_extension, save_extension, validate_extension, ExtensionDocument},
    generator::{generate_project, preview_project, GenerationPreview, GenerationResult},
    recovery::{diagnose_project, export_support_bundle, ProjectHealth},
    secret::{KeyringSecretStore, SecretStore},
    workflow::{
        cancel_workflow, start_workflow, workflow_definitions, WorkflowDefinition, WorkflowState,
        WorkflowTask,
    },
    workspace::{
        create_project, duplicate_project, open_project, save_project, ExplorerLayout,
        ProjectSession, RecentProject, WorkspaceRepository,
    },
};

pub struct SecretState(pub KeyringSecretStore);

#[derive(Default)]
pub struct MigrationState(pub Mutex<HashMap<String, (DatabaseConfig, MigrationPlan)>>);

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
        phase: "Phase 6 - Auth, Permissions & Preview",
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
pub async fn plan_sqlite_schema_changes(
    config: DatabaseConfig,
    operations: Vec<SchemaOperation>,
    app: tauri::AppHandle,
) -> CommandResponse<MigrationPlan> {
    let result = SqliteAdapter
        .plan_schema_changes(&config, &operations)
        .await;
    if let Ok(plan) = &result {
        let state = app.state::<MigrationState>();
        match state.0.lock() {
            Ok(mut plans) => {
                plans.retain(|_, (existing, _)| existing.id != config.id);
                plans.insert(plan.id.clone(), (config, plan.clone()));
            }
            Err(_) => return CommandResponse::from_result(Err(AppError::Internal)),
        };
    }
    CommandResponse::from_result(result)
}

#[tauri::command]
pub async fn apply_sqlite_schema_plan(
    plan_id: String,
    confirmation_token: Option<String>,
    app: tauri::AppHandle,
) -> CommandResponse<ApplyResult> {
    let state = app.state::<MigrationState>();
    let stored = match state.0.lock() {
        Ok(plans) => plans.get(&plan_id).cloned(),
        Err(_) => return CommandResponse::from_result(Err(AppError::Internal)),
    };
    let Some((config, plan)) = stored else {
        return CommandResponse::from_result(Err(AppError::NotFound));
    };
    let result = SqliteAdapter
        .apply_schema_changes(&config, &plan, confirmation_token.as_deref())
        .await;
    if result.is_ok() {
        if let Ok(mut plans) = state.0.lock() {
            plans.remove(&plan_id);
        }
    }
    CommandResponse::from_result(result)
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

#[tauri::command]
pub fn load_extension_file(
    project_path: String,
    relative_path: String,
    language: String,
) -> CommandResponse<ExtensionDocument> {
    CommandResponse::from_result(load_extension(
        Path::new(&project_path),
        &relative_path,
        &language,
    ))
}

#[tauri::command]
pub fn validate_extension_file(
    relative_path: String,
    language: String,
    content: String,
) -> CommandResponse<ExtensionDocument> {
    CommandResponse::from_result(validate_extension(&relative_path, &language, content))
}

#[tauri::command]
pub fn save_extension_file(
    project_path: String,
    relative_path: String,
    language: String,
    content: String,
    format: bool,
) -> CommandResponse<ExtensionDocument> {
    CommandResponse::from_result(save_extension(
        Path::new(&project_path),
        &relative_path,
        &language,
        content,
        format,
    ))
}

#[tauri::command]
pub fn list_workflow_definitions() -> CommandResponse<Vec<WorkflowDefinition>> {
    CommandResponse::from_result(Ok(workflow_definitions()))
}

#[tauri::command]
pub fn list_workflow_tasks(state: State<'_, WorkflowState>) -> CommandResponse<Vec<WorkflowTask>> {
    CommandResponse::from_result(state.tasks())
}

#[tauri::command]
pub fn start_registered_workflow(
    project_path: String,
    workflow_id: String,
    working_directory: Option<String>,
    app: tauri::AppHandle,
) -> CommandResponse<WorkflowTask> {
    CommandResponse::from_result(start_workflow(
        app,
        project_path,
        workflow_id,
        working_directory,
    ))
}

#[tauri::command]
pub fn cancel_registered_workflow(
    task_id: String,
    state: State<'_, WorkflowState>,
) -> CommandResponse<()> {
    CommandResponse::from_result(cancel_workflow(&state, &task_id))
}

#[tauri::command]
pub fn diagnose_project_command(project_path: String) -> CommandResponse<ProjectHealth> {
    CommandResponse::from_result(diagnose_project(Path::new(&project_path)))
}

#[tauri::command]
pub fn recover_project_command(
    project_path: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    let path = Path::new(&project_path);
    CommandResponse::from_result(recover_blueprint(path).and_then(|_| open_project(&state, path)))
}

#[tauri::command]
pub fn export_support_bundle_command(
    project_path: String,
    destination_directory: String,
    state: State<'_, WorkflowState>,
) -> CommandResponse<String> {
    CommandResponse::from_result(state.tasks().and_then(|tasks| {
        export_support_bundle(
            Path::new(&project_path),
            Path::new(&destination_directory),
            &tasks,
        )
    }))
}

#[tauri::command]
pub fn preview_generation_command(
    project_path: String,
    target_directory: String,
) -> CommandResponse<GenerationPreview> {
    CommandResponse::from_result(preview_project(
        Path::new(&project_path),
        Path::new(&target_directory),
    ))
}

#[tauri::command]
pub fn generate_project_command(
    project_path: String,
    target_directory: String,
) -> CommandResponse<GenerationResult> {
    CommandResponse::from_result(generate_project(
        Path::new(&project_path),
        Path::new(&target_directory),
    ))
}
