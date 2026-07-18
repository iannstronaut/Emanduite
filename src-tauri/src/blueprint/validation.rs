use std::{
    collections::HashSet,
    path::{Component, Path},
};

use schemars::schema_for;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::model::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValidationDiagnostic {
    pub path: String,
    pub code: String,
    pub message: String,
}

fn diagnostic(path: impl Into<String>, code: &str, message: &str) -> ValidationDiagnostic {
    ValidationDiagnostic {
        path: path.into(),
        code: code.into(),
        message: message.into(),
    }
}

pub fn validate_blueprint(blueprint: &Blueprint) -> Vec<ValidationDiagnostic> {
    let mut result = Vec::new();
    if blueprint.schema_version != CURRENT_SCHEMA_VERSION {
        result.push(diagnostic(
            "schemaVersion",
            "unsupported_version",
            "Blueprint schema version is not supported",
        ));
    }
    validate_uuid(&blueprint.project_id, "projectId", &mut result);
    if blueprint.project_name.trim().is_empty() {
        result.push(diagnostic(
            "projectName",
            "required",
            "Project name is required",
        ));
    }

    let mut ids = HashSet::new();
    let mut database_ids = HashSet::new();
    let mut table_ids = HashSet::new();
    let mut column_ids = HashSet::new();
    collect_database_refs(
        &blueprint.databases.main,
        &mut database_ids,
        &mut table_ids,
        &mut column_ids,
    );
    for side in &blueprint.databases.sides {
        collect_database_refs(side, &mut database_ids, &mut table_ids, &mut column_ids);
    }
    check_unique_id(&blueprint.project_id, "projectId", &mut ids, &mut result);
    validate_database(
        &blueprint.databases.main,
        "databases.main",
        false,
        &mut ids,
        &mut result,
    );
    for (index, side) in blueprint.databases.sides.iter().enumerate() {
        validate_database(
            side,
            &format!("databases.sides[{index}]"),
            true,
            &mut ids,
            &mut result,
        );
    }
    for (key, entity) in &blueprint.entities {
        let path = format!("entities.{key}");
        check_unique_id(&entity.id, &format!("{path}.id"), &mut ids, &mut result);
        if !database_ids.contains(entity.database_id.as_str()) {
            result.push(diagnostic(
                format!("{path}.databaseId"),
                "unknown_database",
                "Entity database ID does not exist",
            ));
        }
        if !table_ids.contains(entity.table_id.as_str()) {
            result.push(diagnostic(
                format!("{path}.tableId"),
                "unknown_table",
                "Entity table ID does not exist",
            ));
        }
        for (field_key, field) in &entity.fields {
            check_unique_id(
                &field.id,
                &format!("{path}.fields.{field_key}.id"),
                &mut ids,
                &mut result,
            );
            if !column_ids.contains(field.column_id.as_str()) {
                result.push(diagnostic(
                    format!("{path}.fields.{field_key}.columnId"),
                    "unknown_column",
                    "Entity field column ID does not exist",
                ));
            }
            let mut option_values = HashSet::new();
            if field
                .options
                .iter()
                .any(|option| !option_values.insert(option.value.as_str()))
            {
                result.push(diagnostic(
                    format!("{path}.fields.{field_key}.options"),
                    "duplicate_option",
                    "Field option values must be unique",
                ));
            }
            if let Some(relation) = &field.relation_display {
                let target = blueprint
                    .entities
                    .values()
                    .find(|candidate| candidate.id == relation.target_entity_id);
                if let Some(target) = target {
                    if !target
                        .fields
                        .values()
                        .any(|candidate| candidate.id == relation.display_field_id)
                    {
                        result.push(diagnostic(
                            format!("{path}.fields.{field_key}.relationDisplay.displayFieldId"),
                            "unknown_entity_field",
                            "Relation display field does not exist on the target entity",
                        ));
                    }
                } else {
                    result.push(diagnostic(
                        format!("{path}.fields.{field_key}.relationDisplay.targetEntityId"),
                        "unknown_entity",
                        "Relation display target entity does not exist",
                    ));
                }
            }
        }
    }
    let mut resource_ids = HashSet::new();
    for (key, resource) in &blueprint.resources {
        check_unique_id(
            &resource.id,
            &format!("resources.{key}.id"),
            &mut ids,
            &mut result,
        );
        if !safe_key(&resource.key) {
            result.push(diagnostic(
                format!("resources.{key}.key"),
                "invalid_resource_key",
                "Resource key must use safe stable characters",
            ));
        }
        if key != &resource.key {
            result.push(diagnostic(
                format!("resources.{key}.key"),
                "key_mismatch",
                "Resource map key must match its declared key",
            ));
        }
        resource_ids.insert(resource.id.as_str());
        for action in &resource.actions {
            if !safe_key(action) {
                result.push(diagnostic(
                    format!("resources.{key}.actions"),
                    "invalid_action",
                    "Permission actions must use safe stable keys",
                ));
            }
        }
    }
    for (key, role) in &blueprint.roles {
        let path = format!("roles.{key}");
        check_unique_id(&role.id, &format!("{path}.id"), &mut ids, &mut result);
        if !safe_key(&role.key) {
            result.push(diagnostic(
                format!("{path}.key"),
                "invalid_role_key",
                "Role key must use safe stable characters",
            ));
        }
        if key != &role.key {
            result.push(diagnostic(
                format!("{path}.key"),
                "key_mismatch",
                "Role map key must match its declared key",
            ));
        }
        for (resource_id, actions) in &role.permissions {
            let resource = blueprint
                .resources
                .values()
                .find(|resource| resource.id == *resource_id);
            if resource.is_none() {
                result.push(diagnostic(
                    format!("{path}.permissions.{resource_id}"),
                    "unknown_resource",
                    "Permission references an unknown resource ID",
                ));
            } else if actions
                .iter()
                .any(|action| !resource.unwrap().actions.contains(action))
            {
                result.push(diagnostic(
                    format!("{path}.permissions.{resource_id}"),
                    "unknown_action",
                    "Role permission action is not declared by the resource",
                ));
            }
        }
    }
    let menu_ids: HashSet<_> = blueprint
        .menus
        .iter()
        .map(|menu| menu.id.as_str())
        .collect();
    for (index, menu) in blueprint.menus.iter().enumerate() {
        let path = format!("menus[{index}]");
        check_unique_id(&menu.id, &format!("{path}.id"), &mut ids, &mut result);
        if let Some(resource_id) = &menu.resource_id {
            if !resource_ids.contains(resource_id.as_str()) {
                result.push(diagnostic(
                    format!("{path}.resourceId"),
                    "unknown_resource",
                    "Menu references an unknown resource",
                ));
            }
        }
        if let Some(parent_id) = &menu.parent_id {
            if parent_id == &menu.id || !menu_ids.contains(parent_id.as_str()) {
                result.push(diagnostic(
                    format!("{path}.parentId"),
                    "invalid_menu_parent",
                    "Menu parent must reference a different menu item",
                ));
            }
        }
    }
    for (index, menu) in blueprint.menus.iter().enumerate() {
        let mut visited = HashSet::new();
        let mut current = Some(menu.id.as_str());
        while let Some(id) = current {
            if !visited.insert(id) {
                result.push(diagnostic(
                    format!("menus[{index}].parentId"),
                    "menu_cycle",
                    "Menu hierarchy must not contain a cycle",
                ));
                break;
            }
            current = blueprint
                .menus
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.parent_id.as_deref());
        }
    }
    for (key, extension) in &blueprint.extensions {
        let path = format!("extensions.{key}");
        check_unique_id(&extension.id, &format!("{path}.id"), &mut ids, &mut result);
        let extension_path = Path::new(&extension.path);
        if extension_path.is_absolute()
            || extension_path.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || extension.path.trim().is_empty()
        {
            result.push(diagnostic(
                format!("{path}.path"),
                "unsafe_extension_path",
                "Extension path must be relative and remain inside the extension root",
            ));
        }
    }
    if let Some(auth) = &blueprint.auth {
        if !database_ids.contains(auth.database_id.as_str()) {
            result.push(diagnostic(
                "auth.databaseId",
                "unknown_database",
                "Auth database does not exist",
            ));
        }
        let entity = blueprint
            .entities
            .values()
            .find(|entity| entity.id == auth.user_entity_id);
        if let Some(entity) = entity {
            let fields: HashSet<_> = entity
                .fields
                .values()
                .map(|field| field.id.as_str())
                .collect();
            for (path, field_id) in [
                ("auth.externalIdFieldId", &auth.external_id_field_id),
                ("auth.identifierFieldId", &auth.identifier_field_id),
                ("auth.passwordFieldId", &auth.password_field_id),
            ] {
                if !fields.contains(field_id.as_str()) {
                    result.push(diagnostic(
                        path,
                        "unknown_entity_field",
                        "Auth field binding does not exist",
                    ));
                }
            }
        } else {
            result.push(diagnostic(
                "auth.userEntityId",
                "unknown_entity",
                "Auth user entity does not exist",
            ));
        }
    }
    result
}

fn collect_database_refs<'a>(
    database: &'a DatabaseConfig,
    database_ids: &mut HashSet<&'a str>,
    table_ids: &mut HashSet<&'a str>,
    column_ids: &mut HashSet<&'a str>,
) {
    database_ids.insert(&database.id);
    for table in &database.tables {
        table_ids.insert(&table.id);
        for column in &table.columns {
            column_ids.insert(&column.id);
        }
    }
}

fn safe_key(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
}

fn validate_database(
    db: &DatabaseConfig,
    path: &str,
    side: bool,
    ids: &mut HashSet<String>,
    result: &mut Vec<ValidationDiagnostic>,
) {
    check_unique_id(&db.id, &format!("{path}.id"), ids, result);
    if db.name.trim().is_empty() {
        result.push(diagnostic(
            format!("{path}.name"),
            "required",
            "Database name is required",
        ));
    }
    if !db.capabilities.contains(&Capability::Read) {
        result.push(diagnostic(
            format!("{path}.capabilities"),
            "read_required",
            "Every database must allow read access",
        ));
    }
    if side && db.capabilities.contains(&Capability::Schema) {
        result.push(diagnostic(
            format!("{path}.capabilities"),
            "side_schema_denied",
            "Side database schema is always read-only",
        ));
    }
    match (&db.provider, &db.connection) {
        (DatabaseProvider::Sqlite, ConnectionConfig::Sqlite { path: sqlite_path })
            if sqlite_path.trim().is_empty() =>
        {
            result.push(diagnostic(
                format!("{path}.connection.path"),
                "required",
                "SQLite path is required",
            ))
        }
        (DatabaseProvider::Sqlite, ConnectionConfig::Sqlite { .. })
        | (
            DatabaseProvider::Postgresql | DatabaseProvider::Mysql,
            ConnectionConfig::Server { .. },
        ) => {}
        _ => result.push(diagnostic(
            format!("{path}.connection"),
            "provider_mismatch",
            "Connection kind does not match provider",
        )),
    }
    for (table_index, table) in db.tables.iter().enumerate() {
        let table_path = format!("{path}.tables[{table_index}]");
        check_unique_id(&table.id, &format!("{table_path}.id"), ids, result);
        for (column_index, column) in table.columns.iter().enumerate() {
            check_unique_id(
                &column.id,
                &format!("{table_path}.columns[{column_index}].id"),
                ids,
                result,
            );
        }
        for (fk_index, fk) in table.foreign_keys.iter().enumerate() {
            check_unique_id(
                &fk.id,
                &format!("{table_path}.foreignKeys[{fk_index}].id"),
                ids,
                result,
            );
        }
        for (index_index, index) in table.indexes.iter().enumerate() {
            check_unique_id(
                &index.id,
                &format!("{table_path}.indexes[{index_index}].id"),
                ids,
                result,
            );
        }
    }
}

fn validate_uuid(value: &str, path: &str, result: &mut Vec<ValidationDiagnostic>) {
    if Uuid::parse_str(value).is_err() {
        result.push(diagnostic(
            path,
            "invalid_uuid",
            "A stable UUID is required",
        ));
    }
}

fn check_unique_id(
    value: &str,
    path: &str,
    ids: &mut HashSet<String>,
    result: &mut Vec<ValidationDiagnostic>,
) {
    validate_uuid(value, path, result);
    if !ids.insert(value.to_owned()) {
        result.push(diagnostic(
            path,
            "duplicate_id",
            "Stable IDs must be globally unique within a blueprint",
        ));
    }
}

pub fn blueprint_json_schema() -> Value {
    serde_json::to_value(schema_for!(Blueprint)).expect("Blueprint schema is serializable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_new_sqlite_blueprint() {
        let blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        assert_eq!(validate_blueprint(&blueprint), Vec::new());
    }

    #[test]
    fn rejects_duplicate_stable_ids() {
        let mut blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        blueprint.databases.main.id = blueprint.project_id.clone();
        assert!(validate_blueprint(&blueprint)
            .iter()
            .any(|item| item.code == "duplicate_id"));
    }

    #[test]
    fn exports_json_schema() {
        let schema = blueprint_json_schema();
        assert!(schema.get("definitions").is_some() || schema.get("$defs").is_some());
    }

    #[test]
    fn rejects_unknown_permission_action_and_menu_cycle() {
        let mut blueprint = Blueprint::new_sqlite("Demo", "demo.db");
        let resource_id = Uuid::new_v4().to_string();
        blueprint.resources.insert(
            "users".into(),
            ResourceConfig {
                id: resource_id.clone(),
                key: "users".into(),
                resource_type: "entity".into(),
                actions: ["read".into()].into_iter().collect(),
            },
        );
        blueprint.roles.insert(
            "admin".into(),
            RoleConfig {
                id: Uuid::new_v4().to_string(),
                key: "admin".into(),
                label: "Admin".into(),
                permissions: [(resource_id, ["export".into()].into_iter().collect())]
                    .into_iter()
                    .collect(),
            },
        );
        let first = Uuid::new_v4().to_string();
        let second = Uuid::new_v4().to_string();
        blueprint.menus = vec![
            MenuItem {
                id: first.clone(),
                label: "First".into(),
                resource_id: None,
                parent_id: Some(second.clone()),
                order: 0,
            },
            MenuItem {
                id: second,
                label: "Second".into(),
                resource_id: None,
                parent_id: Some(first),
                order: 1,
            },
        ];

        let diagnostics = validate_blueprint(&blueprint);
        assert!(diagnostics.iter().any(|item| item.code == "unknown_action"));
        assert!(diagnostics.iter().any(|item| item.code == "menu_cycle"));
    }
}
