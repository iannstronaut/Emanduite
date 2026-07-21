use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Blueprint {
    pub schema_version: u32,
    pub project_id: String,
    pub project_name: String,
    pub generated_with: GeneratedWith,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_directory: Option<String>,
    pub databases: Databases,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    #[serde(default)]
    pub entities: BTreeMap<String, EntityConfig>,
    #[serde(default)]
    pub resources: BTreeMap<String, ResourceConfig>,
    #[serde(default)]
    pub roles: BTreeMap<String, RoleConfig>,
    #[serde(default)]
    pub menus: Vec<MenuItem>,
    #[serde(default)]
    pub extensions: BTreeMap<String, ExtensionConfig>,
    #[serde(default)]
    pub global: GlobalConfig,
}

impl Blueprint {
    pub fn new_sqlite(project_name: impl Into<String>, sqlite_path: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            project_id: Uuid::new_v4().to_string(),
            project_name: project_name.into(),
            generated_with: GeneratedWith {
                emanduite: env!("CARGO_PKG_VERSION").into(),
                template: "desktop-foundation".into(),
            },
            target_directory: None,
            databases: Databases {
                main: DatabaseConfig {
                    id: Uuid::new_v4().to_string(),
                    name: "Main SQLite".into(),
                    provider: DatabaseProvider::Sqlite,
                    capabilities: [
                        Capability::Read,
                        Capability::Create,
                        Capability::Update,
                        Capability::Delete,
                        Capability::Schema,
                    ]
                    .into_iter()
                    .collect(),
                    connection: ConnectionConfig::Sqlite {
                        path: sqlite_path.into(),
                    },
                    secret_ref: None,
                    tables: Vec::new(),
                },
                sides: Vec::new(),
            },
            auth: None,
            entities: BTreeMap::new(),
            resources: BTreeMap::new(),
            roles: BTreeMap::new(),
            menus: Vec::new(),
            extensions: BTreeMap::new(),
            global: GlobalConfig::default(),
        }
    }

    /// Adds the first usable CRUD/auth configuration for the SQLite system schema.
    /// This is a no-op when the project already has custom entities or auth.
    pub fn bootstrap_default_admin_configuration(&mut self) -> bool {
        if !self.entities.is_empty() || self.auth.is_some() {
            return false;
        }
        let Some(table) = self
            .databases
            .main
            .tables
            .iter()
            .find(|table| table.name == "mst_users")
        else {
            return false;
        };
        let columns = table
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column.id.clone()))
            .collect::<BTreeMap<_, _>>();
        let (Some(id), Some(email), Some(password)) = (
            columns.get("id"),
            columns.get("email"),
            columns.get("password"),
        ) else {
            return false;
        };
        let database_id = self.databases.main.id.clone();
        let entity_id = Uuid::new_v4().to_string();
        let field =
            |column_id: &str, control: &str, show_in_list, show_in_view, show_in_form, required| {
                EntityFieldConfig {
                    id: Uuid::new_v4().to_string(),
                    column_id: column_id.into(),
                    control: control.into(),
                    show_in_list,
                    show_in_view,
                    show_in_form,
                    required,
                    validation: Vec::new(),
                    options: Vec::new(),
                    relation_display: None,
                }
            };
        let id_field = field(id, "hidden", false, true, false, false);
        let email_field = field(email, "text", true, true, true, true);
        let password_field = field(password, "hidden", false, false, false, true);
        let mut fields = BTreeMap::from([
            ("id".into(), id_field.clone()),
            ("email".into(), email_field.clone()),
            ("password".into(), password_field.clone()),
        ]);
        if let Some(name) = columns.get("name") {
            fields.insert("name".into(), field(name, "text", true, true, true, true));
        }
        if let Some(created_at) = columns.get("created_at") {
            fields.insert(
                "created_at".into(),
                field(created_at, "date", false, true, false, false),
            );
        }
        self.entities.insert(
            "users".into(),
            EntityConfig {
                id: entity_id.clone(),
                label: Some("Users".into()),
                database_id: database_id.clone(),
                table_id: table.id.clone(),
                fields,
            },
        );
        let resource_id = Uuid::new_v4().to_string();
        self.resources.insert(
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
        self.roles.insert(
            "superadmin".into(),
            RoleConfig {
                id: Uuid::new_v4().to_string(),
                key: "superadmin".into(),
                label: "Superadmin".into(),
                permissions: BTreeMap::from([(
                    resource_id,
                    ["read", "create", "update", "delete"]
                        .into_iter()
                        .map(String::from)
                        .collect::<BTreeSet<_>>(),
                )]),
            },
        );
        self.auth = Some(AuthConfig {
            database_id,
            user_entity_id: entity_id,
            external_id_field_id: id_field.id,
            identifier_field_id: email_field.id,
            password_field_id: password_field.id,
            registration_policy: RegistrationPolicy::Disabled,
            password_login: true,
        });
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedWith {
    pub emanduite: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Databases {
    pub main: DatabaseConfig,
    #[serde(default)]
    pub sides: Vec<DatabaseConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseConfig {
    pub id: String,
    pub name: String,
    pub provider: DatabaseProvider,
    pub capabilities: BTreeSet<Capability>,
    pub connection: ConnectionConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    #[serde(default)]
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseProvider {
    Sqlite,
    Postgresql,
    Mysql,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "lowercase")]
pub enum Capability {
    Read,
    Create,
    Update,
    Delete,
    Schema,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConnectionConfig {
    Sqlite {
        path: String,
    },
    Server {
        host: String,
        port: u16,
        database: String,
        username: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Table {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKey>,
    #[serde(default)]
    pub indexes: Vec<Index>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    pub id: String,
    pub name: String,
    pub native_type: String,
    pub canonical_type: CanonicalType,
    pub nullable: bool,
    pub primary_key: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CanonicalType {
    Integer,
    Real,
    Decimal,
    Boolean,
    Text,
    Bytes,
    Date,
    DateTime,
    Json,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForeignKey {
    pub id: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Index {
    pub id: String,
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthConfig {
    pub database_id: String,
    pub user_entity_id: String,
    pub external_id_field_id: String,
    pub identifier_field_id: String,
    pub password_field_id: String,
    #[serde(default)]
    pub registration_policy: RegistrationPolicy,
    #[serde(default = "default_true")]
    pub password_login: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationPolicy {
    #[default]
    Disabled,
    InviteOnly,
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntityConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub database_id: String,
    pub table_id: String,
    #[serde(default)]
    pub fields: BTreeMap<String, EntityFieldConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntityFieldConfig {
    pub id: String,
    pub column_id: String,
    pub control: String,
    pub show_in_list: bool,
    pub show_in_view: bool,
    #[serde(default = "default_true")]
    pub show_in_form: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub validation: Vec<ValidationRule>,
    #[serde(default)]
    pub options: Vec<FieldOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_display: Option<RelationDisplay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationRule {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldOption {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationDisplay {
    pub target_entity_id: String,
    pub display_field_id: String,
    #[serde(default)]
    pub missing_behavior: MissingReferenceBehavior,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MissingReferenceBehavior {
    #[default]
    Empty,
    RawValue,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceConfig {
    pub id: String,
    pub key: String,
    pub resource_type: String,
    #[serde(default)]
    pub actions: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleConfig {
    pub id: String,
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub permissions: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionConfig {
    pub id: String,
    pub path: String,
    pub language: String,
    #[serde(default)]
    pub ownership: ExtensionOwnership,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionOwnership {
    #[default]
    UserOwned,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlobalConfig {
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            template: default_template(),
            settings: BTreeMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_template() -> String {
    "default".into()
}
