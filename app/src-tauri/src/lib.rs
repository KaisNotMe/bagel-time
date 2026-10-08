mod accounts;
mod commands;
mod games;
mod mods;

use std::sync::{Arc, Mutex};

use bagel_core::modrinth::Modrinth;
use bagel_core::{Accounts, InstanceStore, Launcher, Paths};

/// Shared by every command. Cheap to clone via `Arc`.
pub struct AppState {
    pub launcher: Launcher,
    pub store: InstanceStore,
    pub accounts: Accounts,
    pub modrinth: Modrinth,
    pub games: games::Games,
    pub login: Mutex<Option<accounts::PendingLogin>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = Paths::default_location().expect("this OS has no application data folder");
    let state = Arc::new(AppState {
        store: InstanceStore::new(&paths),
        accounts: Accounts::new(&paths),
        modrinth: Modrinth::new(),
        launcher: Launcher::new(paths),
        games: games::Games::default(),
        login: Mutex::new(None),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::list_versions,
            commands::list_loader_versions,
            commands::list_instances,
            commands::get_instance,
            commands::create_instance,
            commands::delete_instance,
            commands::open_instance_folder,
            commands::get_settings,
            commands::save_settings,
            commands::launch_instance,
            commands::stop_instance,
            accounts::list_accounts,
            accounts::start_login,
            accounts::open_login_page,
            accounts::cancel_login,
            accounts::set_active_account,
            accounts::remove_account,
            mods::search_modrinth,
            mods::list_mods,
            mods::identify_mods,
            mods::install_mod,
            mods::set_mod_enabled,
            mods::remove_mod,
            mods::check_mod_updates,
            mods::install_modpack,
            mods::import_mrpack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bagel Time");
}
