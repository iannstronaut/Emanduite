pub mod blueprint;
pub mod commands;
pub mod database;
pub mod error;
pub mod extension;
pub mod generator;
pub mod logging;
pub mod recovery;
pub mod secret;
pub mod workflow;
pub mod workspace;

use commands::{MigrationState, SecretState};
use secret::KeyringSecretStore;
use tauri::Manager;
use workflow::WorkflowState;
use workspace::WorkspaceRepository;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state_path = app.path().app_data_dir()?.join("workspace-state.json");
            let repository = WorkspaceRepository::open(state_path)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(repository);
            let workflow_state =
                WorkflowState::open(app.path().app_data_dir()?.join("workflow-history.json"))
                    .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(workflow_state);
            Ok(())
        })
        .manage(SecretState(KeyringSecretStore))
        .manage(MigrationState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::export_blueprint_schema,
            commands::create_sqlite_blueprint,
            commands::validate_blueprint_command,
            commands::save_blueprint_command,
            commands::load_blueprint_command,
            commands::put_secret,
            commands::has_secret,
            commands::delete_secret,
            commands::list_openai_compatible_models,
            commands::generate_openai_compatible_design,
            commands::test_sqlite_connection,
            commands::introspect_sqlite,
            commands::plan_sqlite_schema_changes,
            commands::apply_sqlite_schema_plan,
            commands::create_project_command,
            commands::open_project_command,
            commands::save_project_command,
            commands::duplicate_project_command,
            commands::list_recent_projects,
            commands::get_active_project_path,
            commands::remove_recent_project,
            commands::get_explorer_layout,
            commands::save_explorer_layout,
            commands::load_extension_file,
            commands::validate_extension_file,
            commands::save_extension_file,
            commands::list_workflow_definitions,
            commands::list_workflow_tasks,
            commands::start_registered_workflow,
            commands::cancel_registered_workflow,
            commands::diagnose_project_command,
            commands::recover_project_command,
            commands::export_support_bundle_command,
            commands::preview_generation_command,
            commands::generate_project_command,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Emanduite desktop workspace");
}
