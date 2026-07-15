pub mod blueprint;
pub mod commands;
pub mod database;
pub mod error;
pub mod logging;
pub mod secret;

use commands::SecretState;
use secret::KeyringSecretStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init();
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Emanduite desktop workspace");
}
