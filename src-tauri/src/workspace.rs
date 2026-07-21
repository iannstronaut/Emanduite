use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, Connection};
use uuid::Uuid;

use crate::{
    blueprint::{
        load_blueprint, save_blueprint, Blueprint, CanonicalType, Column, ForeignKey, Table,
    },
    database::{
        sqlite::{validate_existing_sqlite_path, SqliteAdapter},
        DatabaseAdapter, SchemaOperation,
    },
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

pub async fn create_project(
    repository: &WorkspaceRepository,
    directory: &Path,
    name: &str,
    sqlite_path: &Path,
    superadmin_email: &str,
    superadmin_password: &str,
) -> Result<ProjectSession, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::Validation);
    }
    let superadmin_email = validate_superadmin_email(superadmin_email)?;
    if superadmin_password.len() < 12 {
        return Err(AppError::Validation);
    }
    let directory = prepare_project_directory(directory)?;
    let sqlite = validate_existing_sqlite_path(sqlite_path)?;
    let path = directory.join(BLUEPRINT_FILE);
    if path.exists() {
        return Err(AppError::Validation);
    }
    let mut blueprint = Blueprint::new_sqlite(name.trim(), sqlite.to_string_lossy());
    let adapter = SqliteAdapter;
    let current = adapter.introspect(&blueprint.databases.main).await?;
    let is_new_database = current.tables.is_empty();
    if is_new_database {
        let tables = default_system_tables();
        let operations = tables
            .into_iter()
            .map(|table| SchemaOperation::AddTable {
                operation_id: Uuid::new_v4().to_string(),
                table,
            })
            .collect::<Vec<_>>();
        let plan = adapter
            .plan_schema_changes(&blueprint.databases.main, &operations)
            .await?;
        adapter
            .apply_schema_changes(&blueprint.databases.main, &plan, None)
            .await?;
        let password_hash = bcrypt::hash(superadmin_password, bcrypt::DEFAULT_COST)
            .map_err(|_| AppError::Internal)?;
        seed_superadmin(&blueprint.databases.main, &superadmin_email, &password_hash).await?;
    }
    blueprint.databases.main.tables = adapter.introspect(&blueprint.databases.main).await?.tables;
    if is_new_database {
        blueprint.bootstrap_default_admin_configuration();
    }
    save_blueprint(&path, &blueprint)?;
    let session = ProjectSession {
        path: path.to_string_lossy().into_owned(),
        blueprint,
    };
    repository.remember(&session)?;
    Ok(session)
}

fn validate_superadmin_email(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.len() > 254 || !value.contains('@') || value.starts_with('@') || value.ends_with('@') {
        return Err(AppError::Validation);
    }
    Ok(value.to_owned())
}

async fn seed_superadmin(
    config: &crate::blueprint::DatabaseConfig,
    email: &str,
    password_hash: &str,
) -> Result<(), AppError> {
    let crate::blueprint::ConnectionConfig::Sqlite { path } = &config.connection else {
        return Err(AppError::Validation);
    };
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    let mut transaction = connection.begin().await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS mst_users_email_unique ON mst_users (email)")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO mst_roles (name) VALUES (?)")
        .bind("Superadmin")
        .execute(&mut *transaction)
        .await?;
    let role_id: i64 = sqlx::query_scalar("SELECT id FROM mst_roles WHERE name = ?")
        .bind("Superadmin")
        .fetch_one(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO mst_users (name, email, password, role_id) VALUES (?, ?, ?, ?)")
        .bind("Superadmin")
        .bind(email)
        .bind(password_hash)
        .bind(role_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    connection.close().await?;
    Ok(())
}

fn default_system_tables() -> Vec<Table> {
    let column = |name: &str,
                  native_type: &str,
                  canonical_type,
                  nullable: bool,
                  primary_key: bool,
                  default_value: Option<&str>| Column {
        id: Uuid::new_v4().to_string(),
        name: name.into(),
        native_type: native_type.into(),
        canonical_type,
        nullable,
        primary_key,
        default_value: default_value.map(str::to_owned),
    };
    let table = |name: &str, columns: Vec<Column>, foreign_keys: Vec<ForeignKey>| Table {
        id: Uuid::new_v4().to_string(),
        name: name.into(),
        columns,
        foreign_keys,
        indexes: Vec::new(),
    };
    let foreign_key = |from_column: &str, to_table: &str| ForeignKey {
        id: Uuid::new_v4().to_string(),
        from_column: from_column.into(),
        to_table: to_table.into(),
        to_column: "id".into(),
        on_update: Some("CASCADE".into()),
        on_delete: Some("SET NULL".into()),
    };

    vec![
        table(
            "mst_roles",
            vec![
                column("id", "INTEGER", CanonicalType::Integer, false, true, None),
                column("name", "TEXT", CanonicalType::Text, false, false, None),
            ],
            vec![],
        ),
        table(
            "mst_users",
            vec![
                column("id", "INTEGER", CanonicalType::Integer, false, true, None),
                column("name", "TEXT", CanonicalType::Text, false, false, None),
                column("email", "TEXT", CanonicalType::Text, false, false, None),
                column("password", "TEXT", CanonicalType::Text, false, false, None),
                column(
                    "role_id",
                    "INTEGER",
                    CanonicalType::Integer,
                    true,
                    false,
                    None,
                ),
                column(
                    "created_at",
                    "DATETIME",
                    CanonicalType::DateTime,
                    false,
                    false,
                    Some("CURRENT_TIMESTAMP"),
                ),
            ],
            vec![foreign_key("role_id", "mst_roles")],
        ),
        table(
            "sys_resources",
            vec![
                column("id", "TEXT", CanonicalType::Text, false, true, None),
                column("key", "TEXT", CanonicalType::Text, false, false, None),
            ],
            vec![],
        ),
        table(
            "sys_permissions",
            vec![
                column("id", "TEXT", CanonicalType::Text, false, true, None),
                column("role_key", "TEXT", CanonicalType::Text, false, false, None),
                column(
                    "resource_key",
                    "TEXT",
                    CanonicalType::Text,
                    false,
                    false,
                    None,
                ),
                column("action", "TEXT", CanonicalType::Text, false, false, None),
            ],
            vec![],
        ),
        table(
            "sys_audit_logs",
            vec![
                column("id", "TEXT", CanonicalType::Text, false, true, None),
                column("subject_id", "TEXT", CanonicalType::Text, true, false, None),
                column(
                    "resource_key",
                    "TEXT",
                    CanonicalType::Text,
                    false,
                    false,
                    None,
                ),
                column("action", "TEXT", CanonicalType::Text, false, false, None),
                column("outcome", "TEXT", CanonicalType::Text, false, false, None),
                column(
                    "created_at",
                    "DATETIME",
                    CanonicalType::DateTime,
                    false,
                    false,
                    Some("CURRENT_TIMESTAMP"),
                ),
            ],
            vec![],
        ),
    ]
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
        let created = create_project(
            &repository,
            &project_directory,
            "Demo",
            &sqlite,
            "admin@example.test",
            "A-strong-password-123",
        )
        .await
        .unwrap();
        assert!(crate::blueprint::validate_blueprint(&created.blueprint).is_empty());
        let tables = created
            .blueprint
            .databases
            .main
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            tables,
            [
                "mst_roles",
                "mst_users",
                "sys_audit_logs",
                "sys_permissions",
                "sys_resources",
            ]
            .into_iter()
            .collect()
        );
        let options = SqliteConnectOptions::new()
            .filename(&sqlite)
            .create_if_missing(false);
        let mut connection = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        let (name, email, password_hash, role): (String, String, String, String) = sqlx::query_as(
            "SELECT u.name, u.email, u.password, r.name FROM mst_users u JOIN mst_roles r ON r.id = u.role_id",
        )
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(
            (name, email, role),
            (
                "Superadmin".into(),
                "admin@example.test".into(),
                "Superadmin".into()
            )
        );
        assert_ne!(password_hash, "A-strong-password-123");
        assert!(bcrypt::verify("A-strong-password-123", &password_hash).unwrap());
        connection.close().await.unwrap();
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
        let created = create_project(
            &repository,
            &directory.path().join("one"),
            "One",
            &sqlite,
            "admin@example.test",
            "A-strong-password-123",
        )
        .await
        .unwrap();
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
            "admin@example.test",
            "A-strong-password-123",
        )
        .await
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
