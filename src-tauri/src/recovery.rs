use std::{fs, io::Write, path::Path};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    blueprint::{
        load_blueprint, load_recovery_snapshot, validate_blueprint, validate_blueprint_path,
        Blueprint, ConnectionConfig,
    },
    error::AppError,
    workflow::WorkflowTask,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Recoverable,
    Corrupt,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealth {
    pub status: HealthStatus,
    pub recovery_available: bool,
    pub diagnostics: Vec<ProjectDiagnostic>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SupportBundle {
    format_version: u32,
    created_at: String,
    application: ApplicationSummary,
    platform: PlatformSummary,
    project: Option<ProjectSummary>,
    health: ProjectHealth,
    workflows: Vec<WorkflowSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationSummary {
    name: &'static str,
    version: &'static str,
    blueprint_schema_version: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformSummary {
    operating_system: &'static str,
    architecture: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    project_id: String,
    project_name: String,
    schema_version: u32,
    template: String,
    main_provider: String,
    main_table_count: usize,
    entity_count: usize,
    resource_count: usize,
    role_count: usize,
    menu_count: usize,
    extension_count: usize,
    auth_enabled: bool,
    project_path: &'static str,
    database_path: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowSummary {
    id: String,
    workflow_id: String,
    status: crate::workflow::WorkflowStatus,
    started_at: String,
    finished_at: Option<String>,
    exit_code: Option<i32>,
    message: Option<String>,
    working_directory: &'static str,
    output_line_count: usize,
}

pub fn diagnose_project(path: &Path) -> Result<ProjectHealth, AppError> {
    validate_blueprint_path(path)?;
    if !path.exists() {
        return Ok(ProjectHealth {
            status: HealthStatus::Missing,
            recovery_available: load_recovery_snapshot(path).is_ok(),
            diagnostics: vec![diagnostic(
                "blueprint_missing",
                DiagnosticSeverity::Error,
                "The project blueprint file is missing",
            )],
            checked_at: Utc::now().to_rfc3339(),
        });
    }
    match load_blueprint(path) {
        Ok(blueprint) => Ok(diagnose_loaded(path, &blueprint)),
        Err(_) => {
            let recovery_available = load_recovery_snapshot(path).is_ok();
            Ok(ProjectHealth {
                status: if recovery_available {
                    HealthStatus::Recoverable
                } else {
                    HealthStatus::Corrupt
                },
                recovery_available,
                diagnostics: vec![diagnostic(
                    "blueprint_invalid",
                    DiagnosticSeverity::Error,
                    "The project blueprint cannot be parsed or validated",
                )],
                checked_at: Utc::now().to_rfc3339(),
            })
        }
    }
}

fn diagnose_loaded(path: &Path, blueprint: &Blueprint) -> ProjectHealth {
    let mut diagnostics: Vec<_> = validate_blueprint(blueprint)
        .into_iter()
        .map(|item| {
            diagnostic(
                &item.code,
                DiagnosticSeverity::Error,
                &format!("{}: {}", item.path, item.message),
            )
        })
        .collect();
    if let ConnectionConfig::Sqlite { path: sqlite_path } = &blueprint.databases.main.connection {
        if !Path::new(sqlite_path).is_file() {
            diagnostics.push(diagnostic(
                "sqlite_missing",
                DiagnosticSeverity::Error,
                "The configured Main SQLite database is unavailable",
            ));
        }
    }
    if let Some(target) = &blueprint.target_directory {
        if !Path::new(target).is_dir() {
            diagnostics.push(diagnostic(
                "target_directory_missing",
                DiagnosticSeverity::Warning,
                "The configured generated target directory is unavailable",
            ));
        }
    }
    if let Some(root) = path.parent().map(|value| value.join("extensions")) {
        for extension in blueprint.extensions.values() {
            if !root.join(&extension.path).is_file() {
                diagnostics.push(diagnostic(
                    "extension_file_missing",
                    DiagnosticSeverity::Warning,
                    "An extension manifest entry does not have a matching file",
                ));
            }
        }
    }
    let status = if diagnostics
        .iter()
        .any(|item| item.severity == DiagnosticSeverity::Error)
    {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };
    ProjectHealth {
        status,
        recovery_available: load_recovery_snapshot(path).is_ok(),
        diagnostics,
        checked_at: Utc::now().to_rfc3339(),
    }
}

fn diagnostic(code: &str, severity: DiagnosticSeverity, message: &str) -> ProjectDiagnostic {
    ProjectDiagnostic {
        code: code.into(),
        severity,
        message: message.into(),
    }
}

pub fn export_support_bundle(
    project_path: &Path,
    destination_directory: &Path,
    tasks: &[WorkflowTask],
) -> Result<String, AppError> {
    validate_blueprint_path(project_path)?;
    if !destination_directory.is_absolute()
        || destination_directory
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::InvalidPath);
    }
    let destination = destination_directory
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    if !destination.is_dir() {
        return Err(AppError::InvalidPath);
    }
    let health = diagnose_project(project_path)?;
    let project = load_blueprint(project_path)
        .ok()
        .map(|blueprint| project_summary(&blueprint));
    let bundle = SupportBundle {
        format_version: 1,
        created_at: Utc::now().to_rfc3339(),
        application: ApplicationSummary {
            name: "Emanduite",
            version: env!("CARGO_PKG_VERSION"),
            blueprint_schema_version: crate::blueprint::CURRENT_SCHEMA_VERSION,
        },
        platform: PlatformSummary {
            operating_system: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
        },
        project,
        health,
        workflows: tasks.iter().take(100).map(workflow_summary).collect(),
    };
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    let path = destination.join(format!("emanduite-support-{stamp}.json"));
    let temp = destination.join(format!(".support.{}.tmp", Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(&bundle).map_err(|_| AppError::Internal)?;
    let mut file = fs::File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if let Err(error) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(path.to_string_lossy().into_owned())
}

fn project_summary(blueprint: &Blueprint) -> ProjectSummary {
    ProjectSummary {
        project_id: blueprint.project_id.clone(),
        project_name: blueprint.project_name.clone(),
        schema_version: blueprint.schema_version,
        template: blueprint.global.template.clone(),
        main_provider: format!("{:?}", blueprint.databases.main.provider).to_lowercase(),
        main_table_count: blueprint.databases.main.tables.len(),
        entity_count: blueprint.entities.len(),
        resource_count: blueprint.resources.len(),
        role_count: blueprint.roles.len(),
        menu_count: blueprint.menus.len(),
        extension_count: blueprint.extensions.len(),
        auth_enabled: blueprint.auth.is_some(),
        project_path: "[REDACTED_PATH]",
        database_path: "[REDACTED_PATH]",
    }
}

fn workflow_summary(task: &WorkflowTask) -> WorkflowSummary {
    WorkflowSummary {
        id: task.id.clone(),
        workflow_id: task.workflow_id.clone(),
        status: task.status,
        started_at: task.started_at.clone(),
        finished_at: task.finished_at.clone(),
        exit_code: task.exit_code,
        message: task.message.as_deref().map(crate::logging::redact_text),
        working_directory: "[REDACTED_PATH]",
        output_line_count: task.output.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::{recover_blueprint, save_blueprint};

    #[test]
    fn diagnoses_and_recovers_corrupt_blueprint() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("emanduite-project.json");
        let blueprint = Blueprint::new_sqlite("Recovery", "missing.sqlite");
        save_blueprint(&project_path, &blueprint).unwrap();
        fs::write(&project_path, "broken").unwrap();
        let health = diagnose_project(&project_path).unwrap();
        assert_eq!(health.status, HealthStatus::Recoverable);
        assert!(health.recovery_available);
        recover_blueprint(&project_path).unwrap();
        assert!(matches!(
            diagnose_project(&project_path).unwrap().status,
            HealthStatus::Healthy | HealthStatus::Degraded
        ));
    }

    #[test]
    fn support_bundle_redacts_paths_and_excludes_output_content() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("emanduite-project.json");
        let sqlite_path = directory.path().join("secret-name.sqlite");
        fs::write(&sqlite_path, []).unwrap();
        let blueprint = Blueprint::new_sqlite("Support", sqlite_path.to_string_lossy());
        save_blueprint(&project_path, &blueprint).unwrap();
        let task = WorkflowTask {
            id: Uuid::new_v4().to_string(),
            workflow_id: "npm-build".into(),
            label: "Build".into(),
            working_directory: directory.path().to_string_lossy().into_owned(),
            status: crate::workflow::WorkflowStatus::Failed,
            started_at: Utc::now().to_rfc3339(),
            finished_at: Some(Utc::now().to_rfc3339()),
            exit_code: Some(1),
            message: Some("Build failed".into()),
            output: vec![crate::workflow::WorkflowOutput {
                sequence: 1,
                stream: crate::workflow::OutputStream::Stderr,
                line: "token:should-never-be-exported".into(),
                timestamp: Utc::now().to_rfc3339(),
            }],
        };
        let bundle = export_support_bundle(&project_path, directory.path(), &[task]).unwrap();
        let content = fs::read_to_string(bundle).unwrap();
        assert!(!content.contains("secret-name.sqlite"));
        assert!(!content.contains("should-never-be-exported"));
        assert!(!content.contains(&directory.path().to_string_lossy().to_string()));
        assert!(content.contains("[REDACTED_PATH]"));
    }
}
