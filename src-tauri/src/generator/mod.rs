mod ownership;
mod render;

use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    blueprint::{
        load_blueprint, save_blueprint, validate_blueprint, validate_blueprint_path, Blueprint,
    },
    error::AppError,
};

pub const TEMPLATE_ID: &str = "next-admin-v1";
pub const TEMPLATE_VERSION: &str = "1.4.0";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Ownership {
    Generated,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestFile {
    pub owner: Ownership,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerationManifest {
    pub format_version: u32,
    pub template_id: String,
    pub template_version: String,
    pub blueprint_hash: String,
    pub files: BTreeMap<String, ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConflict {
    pub path: String,
    pub artifact_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationPreview {
    pub template_id: String,
    pub template_version: String,
    pub target_directory: String,
    pub entity_count: usize,
    pub generated_file_count: usize,
    pub user_file_count: usize,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationResult {
    pub template_id: String,
    pub template_version: String,
    pub target_directory: String,
    pub blueprint_hash: String,
    pub written_file_count: usize,
    pub preserved_file_count: usize,
    pub conflicts: Vec<GenerationConflict>,
    pub manifest: GenerationManifest,
}

#[derive(Debug, Clone)]
struct GeneratedFile {
    path: String,
    owner: Ownership,
    content: String,
}

pub fn preview_project(
    project_path: &Path,
    target_directory: &Path,
) -> Result<GenerationPreview, AppError> {
    let (blueprint, project_root, target) = generation_input(project_path, target_directory)?;
    let files = render::render_project(&blueprint, &project_root)?;
    Ok(GenerationPreview {
        template_id: TEMPLATE_ID.into(),
        template_version: TEMPLATE_VERSION.into(),
        target_directory: target.to_string_lossy().into_owned(),
        entity_count: blueprint.entities.len(),
        generated_file_count: files
            .iter()
            .filter(|file| file.owner == Ownership::Generated)
            .count(),
        user_file_count: files
            .iter()
            .filter(|file| file.owner == Ownership::User)
            .count(),
        files: files.into_iter().map(|file| file.path).collect(),
    })
}

pub fn generate_project(
    project_path: &Path,
    target_directory: &Path,
) -> Result<GenerationResult, AppError> {
    let (blueprint, project_root, target) = generation_input(project_path, target_directory)?;
    let files = render::render_project(&blueprint, &project_root)?;
    let mut completed_blueprint = blueprint.clone();
    completed_blueprint.target_directory = Some(target.to_string_lossy().into_owned());
    completed_blueprint.generated_with.template = format!("{TEMPLATE_ID}@{TEMPLATE_VERSION}");
    completed_blueprint.generated_with.emanduite = env!("CARGO_PKG_VERSION").into();
    let blueprint_bytes =
        serde_json::to_vec(&completed_blueprint).map_err(|_| AppError::Internal)?;
    let blueprint_hash = ownership::hash_bytes(&blueprint_bytes);
    let (manifest, conflicts, written, preserved) = ownership::apply_plan(
        &target,
        &files,
        TEMPLATE_ID,
        TEMPLATE_VERSION,
        &blueprint_hash,
    )?;
    if conflicts.is_empty() {
        save_blueprint(project_path, &completed_blueprint)?;
    } else {
        let mut incomplete_blueprint = blueprint;
        incomplete_blueprint.target_directory = Some(target.to_string_lossy().into_owned());
        save_blueprint(project_path, &incomplete_blueprint)?;
    }
    Ok(GenerationResult {
        template_id: TEMPLATE_ID.into(),
        template_version: TEMPLATE_VERSION.into(),
        target_directory: target.to_string_lossy().into_owned(),
        blueprint_hash,
        written_file_count: written,
        preserved_file_count: preserved,
        conflicts,
        manifest,
    })
}

fn generation_input(
    project_path: &Path,
    target_directory: &Path,
) -> Result<(Blueprint, std::path::PathBuf, std::path::PathBuf), AppError> {
    validate_blueprint_path(project_path)?;
    let project_file = project_path
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    let project_root = project_file
        .parent()
        .ok_or(AppError::InvalidPath)?
        .to_path_buf();
    if !target_directory.is_absolute()
        || target_directory
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::InvalidPath);
    }
    let target = target_directory
        .canonicalize()
        .map_err(|_| AppError::InvalidPath)?;
    if !target.is_dir() || target == project_root || project_root.starts_with(&target) {
        return Err(AppError::InvalidPath);
    }
    let mut blueprint = load_blueprint(&project_file)?;
    if !validate_blueprint(&blueprint).is_empty() {
        return Err(AppError::Validation);
    }
    blueprint.bootstrap_default_admin_configuration();
    fs::create_dir_all(target.join(".emanduite"))?;
    Ok((blueprint, project_root, target))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::blueprint::{
        AuthConfig, CanonicalType, Column, ConnectionConfig, DatabaseProvider, EntityConfig,
        EntityFieldConfig, ExtensionConfig, ExtensionOwnership, RegistrationPolicy, ResourceConfig,
        RoleConfig, Table,
    };

    fn fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let project = root.join("project");
        let target = root.join("generated");
        fs::create_dir_all(project.join("extensions/hooks")).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(
            project.join("extensions/hooks/users.ts"),
            "export const customUserHook = true;\n",
        )
        .unwrap();
        let database_id = uuid::Uuid::new_v4().to_string();
        let table_id = uuid::Uuid::new_v4().to_string();
        let id_column = uuid::Uuid::new_v4().to_string();
        let name_column = uuid::Uuid::new_v4().to_string();
        let mut blueprint = Blueprint::new_sqlite(
            "Generated Fixture",
            root.join("fixture.sqlite").to_string_lossy(),
        );
        blueprint.databases.main.id = database_id.clone();
        blueprint.databases.main.tables = vec![Table {
            id: table_id.clone(),
            name: "users".into(),
            columns: vec![
                Column {
                    id: id_column.clone(),
                    name: "id".into(),
                    native_type: "INTEGER".into(),
                    canonical_type: CanonicalType::Integer,
                    nullable: false,
                    primary_key: true,
                    default_value: None,
                },
                Column {
                    id: name_column.clone(),
                    name: "name".into(),
                    native_type: "TEXT".into(),
                    canonical_type: CanonicalType::Text,
                    nullable: false,
                    primary_key: false,
                    default_value: None,
                },
            ],
            foreign_keys: vec![],
            indexes: vec![],
        }];
        blueprint.entities.insert(
            "users".into(),
            EntityConfig {
                id: uuid::Uuid::new_v4().to_string(),
                database_id,
                table_id,
                label: Some("Users".into()),
                fields: BTreeMap::from([
                    (
                        "id".into(),
                        EntityFieldConfig {
                            id: uuid::Uuid::new_v4().to_string(),
                            column_id: id_column,
                            control: "number".into(),
                            show_in_list: true,
                            show_in_view: true,
                            show_in_form: false,
                            required: true,
                            validation: vec![],
                            options: vec![],
                            relation_display: None,
                        },
                    ),
                    (
                        "name".into(),
                        EntityFieldConfig {
                            id: uuid::Uuid::new_v4().to_string(),
                            column_id: name_column,
                            control: "text".into(),
                            show_in_list: true,
                            show_in_view: true,
                            show_in_form: true,
                            required: true,
                            validation: vec![],
                            options: vec![],
                            relation_display: None,
                        },
                    ),
                ]),
            },
        );
        blueprint.extensions.insert(
            "userHook".into(),
            ExtensionConfig {
                id: uuid::Uuid::new_v4().to_string(),
                path: "hooks/users.ts".into(),
                language: "typescript".into(),
                ownership: ExtensionOwnership::UserOwned,
            },
        );
        let project_file = project.join("emanduite-project.json");
        save_blueprint(&project_file, &blueprint).unwrap();
        (project_file, target)
    }

    #[test]
    fn generates_prisma_and_concrete_crud_idempotently() {
        let directory = tempfile::tempdir().unwrap();
        let (project, target) = fixture(directory.path());
        let preview = preview_project(&project, &target).unwrap();
        assert_eq!(preview.entity_count, 1);
        assert!(preview.files.contains(&"prisma/schema.prisma".into()));
        assert!(preview
            .files
            .contains(&"src/app/(dashboard)/users/page.tsx".into()));
        assert!(preview.files.contains(&"components.json".into()));
        assert!(preview
            .files
            .contains(&"src/components/ui/button.tsx".into()));
        assert!(preview
            .files
            .contains(&"src/components/app-sidebar.tsx".into()));
        let first = generate_project(&project, &target).unwrap();
        assert!(first.conflicts.is_empty());
        let first_manifest = fs::read(target.join(".emanduite/manifest.json")).unwrap();
        let second = generate_project(&project, &target).unwrap();
        assert!(second.conflicts.is_empty());
        assert_eq!(second.written_file_count, 0);
        assert_eq!(
            first_manifest,
            fs::read(target.join(".emanduite/manifest.json")).unwrap()
        );
        let schema = fs::read_to_string(target.join("prisma/schema.prisma")).unwrap();
        assert!(schema.contains("model Users"));
        assert!(schema.contains("id Int @id @default(autoincrement())"));
        assert!(fs::read_to_string(target.join("components.json"))
            .unwrap()
            .contains("\"style\": \"new-york\""));
        assert!(
            fs::read_to_string(target.join("src/features/users/table.tsx"))
                .unwrap()
                .contains("@/components/ui/table")
        );
        assert!(
            fs::read_to_string(target.join("src/features/users/table.tsx"))
                .unwrap()
                .contains("colSpan={3}")
        );
        let sidebar = fs::read_to_string(target.join("src/components/app-sidebar.tsx")).unwrap();
        assert!(sidebar.contains("usePathname"));
        assert!(sidebar.contains("pathname.startsWith"));
    }

    #[test]
    fn preserves_user_extension_and_creates_conflict_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let (project, target) = fixture(directory.path());
        generate_project(&project, &target).unwrap();
        let extension = target.join("src/extensions/user/hooks/users.ts");
        fs::write(&extension, "// custom target change\n").unwrap();
        let generated = target.join("src/app/(dashboard)/users/page.tsx");
        fs::write(&generated, "// manual generated change\n").unwrap();
        let result = generate_project(&project, &target).unwrap();
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(
            fs::read_to_string(extension).unwrap(),
            "// custom target change\n"
        );
        assert_eq!(
            fs::read_to_string(generated).unwrap(),
            "// manual generated change\n"
        );
        assert!(target.join(&result.conflicts[0].artifact_path).is_file());
    }

    #[test]
    fn auth_configuration_generates_server_guards_and_system_schema() {
        let directory = tempfile::tempdir().unwrap();
        let (project, target) = fixture(directory.path());
        let mut blueprint = load_blueprint(&project).unwrap();
        let users = blueprint.entities.get("users").unwrap().clone();
        let identifier = users.fields.get("name").unwrap().id.clone();
        let external = users.fields.get("id").unwrap().id.clone();
        let password = identifier.clone();
        let resource_id = uuid::Uuid::new_v4().to_string();
        blueprint.resources.insert(
            "users".into(),
            ResourceConfig {
                id: resource_id.clone(),
                key: "users".into(),
                resource_type: "entity".into(),
                actions: ["read", "create", "update", "delete"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            },
        );
        blueprint.roles.insert(
            "admin".into(),
            RoleConfig {
                id: uuid::Uuid::new_v4().to_string(),
                key: "admin".into(),
                label: "Admin".into(),
                permissions: BTreeMap::from([(
                    resource_id,
                    ["read", "create", "update", "delete"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                )]),
            },
        );
        blueprint.auth = Some(AuthConfig {
            database_id: users.database_id,
            user_entity_id: users.id,
            external_id_field_id: external,
            identifier_field_id: identifier,
            password_field_id: password,
            registration_policy: RegistrationPolicy::Disabled,
            password_login: true,
        });
        save_blueprint(&project, &blueprint).unwrap();
        generate_project(&project, &target).unwrap();
        let auth_source = fs::read_to_string(target.join("src/auth.ts")).unwrap();
        assert!(auth_source.contains("NEXTAUTH_SECRET"));
        assert!(!auth_source.contains("sysAuthSubject.findUnique"));
        assert!(target.join(".env.local").is_file());
        assert!(target.join("proxy.ts").is_file());
        assert!(fs::read_to_string(target.join("proxy.ts"))
            .unwrap()
            .contains("secret: authSecret"));
        assert!(
            fs::read_to_string(target.join("src/app/(dashboard)/layout.tsx"))
                .unwrap()
                .contains("if (!session?.user) redirect(\"/login\")")
        );
        assert!(
            !fs::read_to_string(target.join("src/app/(dashboard)/layout.tsx"))
                .unwrap()
                .contains("{{session.user")
        );
        assert!(
            fs::read_to_string(target.join("src/features/users/actions.ts"))
                .unwrap()
                .contains("requirePermission")
        );
        let schema = fs::read_to_string(target.join("prisma/schema.prisma")).unwrap();
        assert!(schema.contains("model SysAuditLog"));
        assert_eq!(schema.matches("@@map(\"sys_resources\")").count(), 1);
        assert_eq!(schema.matches("@@map(\"sys_permissions\")").count(), 1);
        assert_eq!(schema.matches("@@map(\"sys_audit_logs\")").count(), 1);
    }

    #[test]
    fn server_provider_matrix_renders_prisma_without_credentials() {
        for (provider, expected) in [
            (DatabaseProvider::Postgresql, "postgresql"),
            (DatabaseProvider::Mysql, "mysql"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let (project, target) = fixture(directory.path());
            let mut blueprint = load_blueprint(&project).unwrap();
            blueprint.databases.main.provider = provider;
            blueprint.databases.main.connection = ConnectionConfig::Server {
                host: "db.example.test".into(),
                port: 5432,
                database: "app".into(),
                username: "app".into(),
            };
            save_blueprint(&project, &blueprint).unwrap();
            generate_project(&project, &target).unwrap();
            assert!(fs::read_to_string(target.join("prisma/schema.prisma"))
                .unwrap()
                .contains(&format!("provider = \"{expected}\"")));
            assert!(fs::read_to_string(target.join(".env.example"))
                .unwrap()
                .contains("DATABASE_URL"));
        }
    }
}
