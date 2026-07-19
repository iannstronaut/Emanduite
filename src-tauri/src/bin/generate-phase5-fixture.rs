use std::{collections::BTreeMap, env, fs, path::PathBuf};

use emanduite_lib::{
    blueprint::{
        save_blueprint, Blueprint, CanonicalType, Column, EntityConfig, EntityFieldConfig, Table,
    },
    generator::generate_project,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("phase 5 fixture generation failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let requested = env::args()
        .nth(1)
        .unwrap_or_else(|| ".phase5-generated".into());
    let root = PathBuf::from(requested);
    let root = if root.is_absolute() {
        root
    } else {
        env::current_dir()?.join(root)
    };
    fs::create_dir_all(&root)?;
    let root = root.canonicalize()?;
    let project = root.join("project");
    let target = root.join("app");
    fs::create_dir_all(&project)?;
    fs::create_dir_all(&target)?;
    let database = project.join("fixture.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&database)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, email TEXT NOT NULL UNIQUE, age INTEGER)")
        .execute(&pool).await?;
    sqlx::query("INSERT OR IGNORE INTO users (id,name,email,age) VALUES (1,'Ada Lovelace','ada@example.test',36)")
        .execute(&pool).await?;
    pool.close().await;

    let database_id = fixture_id(2);
    let table_id = fixture_id(3);
    let id_column = fixture_id(4);
    let name_column = fixture_id(5);
    let email_column = fixture_id(6);
    let age_column = fixture_id(7);
    let mut blueprint = Blueprint::new_sqlite("Phase 5 Generated App", database.to_string_lossy());
    blueprint.project_id = fixture_id(1);
    blueprint.databases.main.id = database_id.clone();
    blueprint.databases.main.tables = vec![Table {
        id: table_id.clone(),
        name: "users".into(),
        columns: vec![
            column(&id_column, "id", CanonicalType::Integer, false, true),
            column(&name_column, "name", CanonicalType::Text, false, false),
            column(&email_column, "email", CanonicalType::Text, false, false),
            column(&age_column, "age", CanonicalType::Integer, true, false),
        ],
        foreign_keys: vec![],
        indexes: vec![],
    }];
    blueprint.entities.insert(
        "users".into(),
        EntityConfig {
            id: fixture_id(8),
            label: Some("Users".into()),
            database_id,
            table_id,
            fields: BTreeMap::from([
                (
                    "id".into(),
                    field(9, &id_column, "number", true, true, false, true),
                ),
                (
                    "name".into(),
                    field(10, &name_column, "text", true, true, true, true),
                ),
                (
                    "email".into(),
                    field(11, &email_column, "text", true, true, true, true),
                ),
                (
                    "age".into(),
                    field(12, &age_column, "number", true, true, true, false),
                ),
            ]),
        },
    );
    let project_file = project.join("emanduite-project.json");
    save_blueprint(&project_file, &blueprint)?;
    let result = generate_project(&project_file, &target)?;
    if !result.conflicts.is_empty() {
        return Err("fixture target contains generation conflicts".into());
    }
    println!("{}", target.to_string_lossy());
    Ok(())
}

fn column(id: &str, name: &str, kind: CanonicalType, nullable: bool, primary_key: bool) -> Column {
    Column {
        id: id.into(),
        name: name.into(),
        native_type: if kind == CanonicalType::Integer {
            "INTEGER"
        } else {
            "TEXT"
        }
        .into(),
        canonical_type: kind,
        nullable,
        primary_key,
        default_value: None,
    }
}

fn field(
    id: u8,
    column_id: &str,
    control: &str,
    show_in_list: bool,
    show_in_view: bool,
    show_in_form: bool,
    required: bool,
) -> EntityFieldConfig {
    EntityFieldConfig {
        id: fixture_id(id),
        column_id: column_id.into(),
        control: control.into(),
        show_in_list,
        show_in_view,
        show_in_form,
        required,
        validation: vec![],
        options: vec![],
        relation_display: None,
    }
}

fn fixture_id(value: u8) -> String {
    format!("00000000-0000-4000-8000-{value:012}")
}
