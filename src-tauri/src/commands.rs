use std::{collections::HashMap, path::Path, sync::Mutex};

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleModelRequest {
    pub base_url: String,
    pub secret_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleDesignRequest {
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_output_tokens: u32,
    pub secret_ref: String,
    pub prompt: String,
    pub schema_context: Value,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelListItem>,
}

#[derive(Debug, Deserialize)]
struct ModelListItem {
    id: String,
}

fn openai_compatible_endpoint(base_url: &str, resource: &str) -> Result<url::Url, AppError> {
    let mut base = url::Url::parse(base_url).map_err(|_| AppError::Validation)?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err(AppError::Validation);
    }
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(resource).map_err(|_| AppError::Validation)
}

fn compact_provider_message(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut end = compact.len().min(320);
    while end > 0 && !compact.is_char_boundary(end) {
        end -= 1;
    }
    compact[..end].to_string()
}

fn content_from_chat_completion(value: &Value) -> Option<String> {
    match value.pointer("/choices/0/message/content") {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .filter_map(|part| match part {
                    Value::String(text) => Some(text.as_str()),
                    Value::Object(_) => part.get("text").and_then(Value::as_str),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.trim().is_empty()).then_some(text)
        }
        _ => value
            .pointer("/choices/0/text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned),
    }
}

fn parse_model_json(content: &str) -> Result<Value, AppError> {
    let trimmed = content.trim();
    let trimmed = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix("```").unwrap_or(trimmed).trim();
    serde_json::from_str(trimmed).map_err(|_| {
        AppError::AiProvider("Provider response was not valid JSON. Try again or choose a model that supports structured JSON output.".into())
    })
}

#[tauri::command]
pub fn get_app_info() -> CommandResponse<AppInfo> {
    CommandResponse::from_result(Ok(AppInfo {
        name: "Emanduite",
        version: env!("CARGO_PKG_VERSION"),
        phase: "Phase 7 - Provider Matrix & Release",
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
pub fn list_openai_compatible_models(
    request: OpenAiCompatibleModelRequest,
    state: State<'_, SecretState>,
) -> CommandResponse<Vec<String>> {
    CommandResponse::from_result(tauri::async_runtime::block_on(async {
        let endpoint = openai_compatible_endpoint(&request.base_url, "models")?;
        let api_key = state.0.get(&request.secret_ref)?;
        let response = reqwest::Client::new()
            .get(endpoint)
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|_| AppError::Internal)?;
        if !response.status().is_success() {
            return Err(AppError::Internal);
        }
        let mut models = response
            .json::<ModelListResponse>()
            .await
            .map_err(|_| AppError::Internal)?
            .data
            .into_iter()
            .map(|model| model.id)
            .filter(|model| !model.trim().is_empty())
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        Ok(models)
    }))
}

#[tauri::command]
pub fn generate_openai_compatible_design(
    request: OpenAiCompatibleDesignRequest,
    state: State<'_, SecretState>,
) -> CommandResponse<Value> {
    CommandResponse::from_result(tauri::async_runtime::block_on(async {
        if request.prompt.trim().is_empty() || request.model.trim().is_empty() {
            return Err(AppError::Validation);
        }
        let endpoint = openai_compatible_endpoint(&request.base_url, "chat/completions")?;
        let api_key = state.0.get(&request.secret_ref)?;
        let response = reqwest::Client::new()
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": request.model.trim(),
                "temperature": request.temperature.clamp(0.0, 2.0),
                "max_tokens": request.max_output_tokens.clamp(256, 8000),
                "stream": false,
                "response_format": { "type": "json_object" },
                "messages": [
                    {
                        "role": "system",
                        "content": "You design additive SQLite database schemas for Emanduite. Return JSON only, with exactly this shape: {title:string,summary:string,assumptions:string[],tables:[{name:string,columns:[{name:string,nativeType:string,nullable:boolean,primaryKey:boolean,defaultValue?:string}],foreignKeys?:[{fromColumn:string,toTable:string,toColumn?:string,onDelete?:string}]}]}. Propose new business tables only. Never rename, alter, or propose these protected system tables: mst_roles, mst_users, sys_resources, sys_permissions, sys_audit_logs. Use lower_snake_case table and column names, SQLite types (INTEGER, TEXT, DECIMAL(12,2), DATETIME, BOOLEAN), and include an id INTEGER primary key on each table."
                    },
                    {
                        "role": "user",
                        "content": format!("Design request:\n{}\n\nCurrent Emanduite schema context (read-only):\n{}", request.prompt.trim(), request.schema_context)
                    }
                ]
            }))
            .send()
            .await
            .map_err(|_| AppError::AiProvider("Unable to reach the configured AI provider.".into()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::AiProvider(format!(
                "AI provider returned HTTP {}. {}",
                status,
                compact_provider_message(&body)
            )));
        }
        let body = response.text().await.map_err(|_| {
            AppError::AiProvider("Unable to read the AI provider response body.".into())
        })?;
        let completed = serde_json::from_str::<Value>(&body).map_err(|_| {
            AppError::AiProvider(format!(
                "AI provider returned a non-JSON response: {}",
                compact_provider_message(&body)
            ))
        })?;
        if let Some(message) = completed.pointer("/error/message").and_then(Value::as_str) {
            return Err(AppError::AiProvider(format!(
                "AI provider error: {message}"
            )));
        }
        let content = content_from_chat_completion(&completed).ok_or_else(|| {
            AppError::AiProvider(format!(
                "AI provider response has no usable chat-completion content: {}",
                compact_provider_message(&body)
            ))
        })?;
        parse_model_json(&content)
    }))
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
    superadmin_email: String,
    superadmin_password: String,
    state: State<'_, WorkspaceRepository>,
) -> CommandResponse<ProjectSession> {
    CommandResponse::from_result(tauri::async_runtime::block_on(create_project(
        &state,
        Path::new(&directory),
        &name,
        Path::new(&sqlite_path),
        &superadmin_email,
        &superadmin_password,
    )))
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
