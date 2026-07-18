use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    blueprint::{load_blueprint, save_blueprint, Blueprint},
    database::sqlite::validate_existing_sqlite_path,
    error::AppError,
};

const BLUEPRINT_FILE: &str = "emanduite-project.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSession {
    pub path: String,
    pub blueprint: Blueprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerLayout {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
    pub selected_table_id: Option<String>,
}

impl Default for ExplorerLayout {
    fn default() -> Self {
        Self {
            pan_x: 32.0,
            pan_y: 32.0,
            zoom: 1.0,
            selected_table_id: None,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceFile {
    #[serde(default)]
    recent_projects: Vec<RecentProject>,
    #[serde(default)]
    active_project_path: Option<String>,
    #[serde(default)]
    explorer_layouts: BTreeMap<String, ExplorerLayout>,
}

pub struct WorkspaceRepository {
    path: PathBuf,
    value: Mutex<WorkspaceFile>,
}

impl WorkspaceRepository {
    pub fn open(path: PathBuf) -> Result<Self, AppError> {
        if !path.is_absolute() {
            return Err(AppError::InvalidPath);
        }
        let value = recover_and_read_state(&path)?;
        Ok(Self {
            path,
            value: Mutex::new(value),
        })
    }

    pub fn recent_projects(&self) -> Result<Vec<RecentProject>, AppError> {
        Ok(self.lock()?.recent_projects.clone())
    }

    pub fn active_project_path(&self) -> Result<Option<String>, AppError> {
        Ok(self.lock()?.active_project_path.clone())
    }

    pub fn remember(&self, session: &ProjectSession) -> Result<(), AppError> {
        let mut value = self.lock()?;
        value
            .recent_projects
            .retain(|item| item.path != session.path);
        value.recent_projects.insert(
            0,
            RecentProject {
                path: session.path.clone(),
                name: session.blueprint.project_name.clone(),
                last_opened_at: Utc::now().to_rfc3339(),
            },
        );
        value.recent_projects.truncate(12);
        value.active_project_path = Some(session.path.clone());
        self.persist(&value)
    }

    pub fn remove_reference(&self, path: &str) -> Result<(), AppError> {
        let mut value = self.lock()?;
        value.recent_projects.retain(|item| item.path != path);
        if value.active_project_path.as_deref() == Some(path) {
            value.active_project_path = None;
        }
        value.explorer_layouts.remove(path);
        self.persist(&value)
    }

    pub fn layout(&self, project_path: &str) -> Result<ExplorerLayout, AppError> {
        Ok(self
            .lock()?
            .explorer_layouts
            .get(project_path)
            .cloned()
            .unwrap_or_default())
    }

    pub fn set_layout(
        &self,
        project_path: &str,
        mut layout: ExplorerLayout,
    ) -> Result<(), AppError> {
        validate_blueprint_file(Path::new(project_path))?;
        layout.zoom = layout.zoom.clamp(0.35, 2.5);
        let mut value = self.lock()?;
        value.explorer_layouts.insert(project_path.into(), layout);
        self.persist(&value)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, WorkspaceFile>, AppError> {
        self.value.lock().map_err(|_| AppError::Internal)
    }

    fn persist(&self, value: &WorkspaceFile) -> Result<(), AppError> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| AppError::Internal)?;
        atomic_write(&self.path, &bytes)
    }
}

pub fn create_project(
    repository: &WorkspaceRepository,
    directory: &Path,
    name: &str,
    sqlite_path: &Path,
) -> Result<ProjectSession, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::Validation);
    }
    let directory = prepare_project_directory(directory)?;
    let sqlite = validate_existing_sqlite_path(sqlite_path)?;
    let path = directory.join(BLUEPRINT_FILE);
    if path.exists() {
        return Err(AppError::Validation);
    }
    let blueprint = Blueprint::new_sqlite(name.trim(), sqlite.to_string_lossy());
    save_blueprint(&path, &blueprint)?;
    let session = ProjectSession {
        path: path.to_string_lossy().into_owned(),
        blueprint,
    };
    repository.remember(&session)?;
    Ok(session)
}

pub fn open_project(
    repository: &WorkspaceRepository,
    path: &Path,
) -> Result<ProjectSession, AppError> {
    let canonical = validate_blueprint_file(path)?;
    let session = ProjectSession {
        path: canonical.to_string_lossy().into_owned(),
        blueprint: load_blueprint(&canonical)?,
    };
    repository.remember(&session)?;
    Ok(session)
}

pub fn save_project(
    repository: &WorkspaceRepository,
    path: &Path,
    blueprint: &Blueprint,
) -> Result<ProjectSession, AppError> {
    let canonical = validate_blueprint_file(path)?;
    save_blueprint(&canonical, blueprint)?;
    let session = ProjectSession {
        path: canonical.to_string_lossy().into_owned(),
        blueprint: blueprint.clone(),
    };
    repository.remember(&session)?;
    Ok(session)
}

pub fn duplicate_project(
    repository: &WorkspaceRepository,
    source: &Path,
    target_directory: &Path,
    name: &str,
) -> Result<ProjectSession, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::Validation);
    }
    let source = validate_blueprint_file(source)?;
    let target_directory = prepare_project_directory(target_directory)?;
    let target = target_directory.join(BLUEPRINT_FILE);
    if target.exists() {
        return Err(AppError::Validation);
    }
    let mut blueprint = load_blueprint(&source)?;
    blueprint.project_id = Uuid::new_v4().to_string();
    blueprint.project_name = name.trim().into();
    save_blueprint(&target, &blueprint)?;
    let session = ProjectSession {
        path: target.to_string_lossy().into_owned(),
        blueprint,
    };
    repository.remember(&session)?;
    Ok(session)
}

fn prepare_project_directory(path: &Path) -> Result<PathBuf, AppError> {
    validate_absolute_path(path)?;
    fs::create_dir_all(path)?;
    let canonical = path.canonicalize().map_err(|_| AppError::InvalidPath)?;
    if !canonical.is_dir() {
        return Err(AppError::InvalidPath);
    }
    Ok(canonical)
}

fn validate_blueprint_file(path: &Path) -> Result<PathBuf, AppError> {
    validate_absolute_path(path)?;
    if path.file_name().and_then(|value| value.to_str()) != Some(BLUEPRINT_FILE) {
        return Err(AppError::InvalidPath);
    }
    let canonical = path.canonicalize().map_err(|_| AppError::InvalidPath)?;
    if !canonical.is_file() {
        return Err(AppError::InvalidPath);
    }
    Ok(canonical)
}

fn validate_absolute_path(path: &Path) -> Result<(), AppError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(AppError::InvalidPath);
    }
    Ok(())
}

fn recover_and_read_state(path: &Path) -> Result<WorkspaceFile, AppError> {
    let previous = path.with_extension("previous");
    if !path.exists() && previous.exists() {
        fs::rename(&previous, path)?;
    }
    if !path.exists() {
        return Ok(WorkspaceFile::default());
    }
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|_| AppError::Validation)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::InvalidPath)?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".workspace-state.{}.tmp", Uuid::new_v4()));
    let previous = path.with_extension("previous");
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if previous.exists() {
        fs::remove_file(&previous)?;
    }
    if path.exists() {
        fs::rename(path, &previous)?;
    }
    if let Err(error) = fs::rename(&temp, path) {
        if previous.exists() {
            let _ = fs::rename(&previous, path);
        }
        return Err(AppError::Io(error));
    }
    if previous.exists() {
        fs::remove_file(previous)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{sqlite::SqliteAdapter, DatabaseAdapter};
    use sqlx::{sqlite::SqliteConnectOptions, Connection};

    async fn sqlite_fixture(directory: &Path) -> PathBuf {
        let path = directory.join("fixture.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        connection.close().await.unwrap();
        path
    }

    #[tokio::test]
    async fn create_save_reopen_and_remove_recent_reference() {
        let directory = tempfile::tempdir().unwrap();
        let repository = WorkspaceRepository::open(directory.path().join("state.json")).unwrap();
        let sqlite = sqlite_fixture(directory.path()).await;
        let project_directory = directory.path().join("project");
        let created = create_project(&repository, &project_directory, "Demo", &sqlite).unwrap();
        let reopened = open_project(&repository, Path::new(&created.path)).unwrap();
        assert_eq!(created.blueprint, reopened.blueprint);
        assert_eq!(repository.recent_projects().unwrap().len(), 1);
        repository.remove_reference(&created.path).unwrap();
        assert!(repository.recent_projects().unwrap().is_empty());
        assert!(Path::new(&created.path).exists());
    }

    #[tokio::test]
    async fn duplicate_gets_new_project_identity() {
        let directory = tempfile::tempdir().unwrap();
        let repository = WorkspaceRepository::open(directory.path().join("state.json")).unwrap();
        let sqlite = sqlite_fixture(directory.path()).await;
        let created =
            create_project(&repository, &directory.path().join("one"), "One", &sqlite).unwrap();
        let duplicate = duplicate_project(
            &repository,
            Path::new(&created.path),
            &directory.path().join("two"),
            "Two",
        )
        .unwrap();
        assert_ne!(created.blueprint.project_id, duplicate.blueprint.project_id);
        assert_eq!(duplicate.blueprint.project_name, "Two");
    }

    #[test]
    fn rejects_relative_project_paths() {
        assert!(matches!(
            prepare_project_directory(Path::new("relative")),
            Err(AppError::InvalidPath)
        ));
    }

    #[test]
    fn layout_is_clamped_and_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join(BLUEPRINT_FILE);
        fs::write(&project, "{}").unwrap();
        let state_path = directory.path().join("state.json");
        let repository = WorkspaceRepository::open(state_path.clone()).unwrap();
        repository
            .set_layout(
                project.to_str().unwrap(),
                ExplorerLayout {
                    zoom: 10.0,
                    ..ExplorerLayout::default()
                },
            )
            .unwrap();
        drop(repository);
        let reopened = WorkspaceRepository::open(state_path).unwrap();
        assert_eq!(
            reopened.layout(project.to_str().unwrap()).unwrap().zoom,
            2.5
        );
    }

    #[tokio::test]
    async fn phase_two_slice_create_connect_introspect_save_and_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let sqlite = directory.path().join("slice.sqlite");
        let options = SqliteConnectOptions::new()
            .filename(&sqlite)
            .create_if_missing(true)
            .foreign_keys(true);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE teams (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE); CREATE TABLE members (id INTEGER PRIMARY KEY, team_id INTEGER NOT NULL, email TEXT NOT NULL, FOREIGN KEY(team_id) REFERENCES teams(id)); CREATE INDEX members_email_idx ON members(email);")
            .execute(&mut connection)
            .await
            .unwrap();
        connection.close().await.unwrap();

        let state_path = directory.path().join("workspace-state.json");
        let repository = WorkspaceRepository::open(state_path.clone()).unwrap();
        let mut session = create_project(
            &repository,
            &directory.path().join("project"),
            "Phase 2 Slice",
            &sqlite,
        )
        .unwrap();
        SqliteAdapter
            .test_connection(&session.blueprint.databases.main)
            .await
            .unwrap();
        let introspection = SqliteAdapter
            .introspect(&session.blueprint.databases.main)
            .await
            .unwrap();
        assert_eq!(introspection.tables.len(), 2);
        session.blueprint.databases.main.tables = introspection.tables;
        let saved =
            save_project(&repository, Path::new(&session.path), &session.blueprint).unwrap();
        let selected = saved.blueprint.databases.main.tables[0].id.clone();
        repository
            .set_layout(
                &saved.path,
                ExplorerLayout {
                    selected_table_id: Some(selected.clone()),
                    ..ExplorerLayout::default()
                },
            )
            .unwrap();
        drop(repository);

        let reopened_repository = WorkspaceRepository::open(state_path).unwrap();
        let reopened = open_project(&reopened_repository, Path::new(&saved.path)).unwrap();
        assert_eq!(saved.blueprint, reopened.blueprint);
        assert_eq!(
            reopened_repository
                .layout(&saved.path)
                .unwrap()
                .selected_table_id,
            Some(selected)
        );
    }
}
