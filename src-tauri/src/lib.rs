pub mod blueprint;
pub mod commands;
pub mod database;
pub mod error;
pub mod logging;
pub mod secret;
pub mod workspace;

use commands::SecretState;
use secret::KeyringSecretStore;
use tauri::Manager;
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
            Ok(())
        })
        .manage(SecretState(KeyringSecretStore))
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
            commands::test_sqlite_connection,
            commands::introspect_sqlite,
            commands::create_project_command,
            commands::open_project_command,
            commands::save_project_command,
            commands::duplicate_project_command,
            commands::list_recent_projects,
            commands::get_active_project_path,
            commands::remove_recent_project,
            commands::get_explorer_layout,
            commands::save_explorer_layout,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Emanduite desktop workspace");
}
