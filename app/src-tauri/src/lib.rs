mod accounts;
mod commands;
mod games;
mod content;
mod servers;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use bagel_core::content::Sources;
use bagel_core::curseforge::CurseForge;
use bagel_core::{Accounts, InstanceStore, Launcher, Paths, Settings};

/// Shared by every command. Cheap to clone via `Arc`.
pub struct AppState {
    pub launcher: Launcher,
    pub store: InstanceStore,
    pub accounts: Accounts,
    pub sources: Sources,
    pub games: games::Games,
    pub login: Mutex<Option<accounts::PendingLogin>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = Paths::default_location().expect("this OS has no application data folder");
    let data_root = paths.root().to_path_buf();
    let settings = tauri::async_runtime::block_on(Settings::load(&paths));
    let state = Arc::new(AppState {
        store: InstanceStore::new(&paths),
        accounts: Accounts::new(&paths),
        sources: Sources {
            curseforge: CurseForge::new(&settings.curseforge_api_key),
            ..Default::default()
        },
        launcher: Launcher::new(paths),
        games: games::Games::default(),
        login: Mutex::new(None),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .setup(move |app| {
            // Instance icons, world icons and screenshots are shown straight
            // from the data folder.
            app.asset_protocol_scope().allow_directory(&data_root, true)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_versions,
            commands::list_loader_versions,
            commands::list_instances,
            commands::get_instance,
            commands::create_instance,
            commands::delete_instance,
            commands::open_instance_folder,
            commands::update_instance,
            commands::change_instance_version,
            commands::duplicate_instance,
            commands::set_instance_icon,
            commands::clear_instance_icon,
            commands::list_worlds,
            commands::list_screenshots,
            commands::delete_screenshot,
            commands::open_screenshot,
            commands::list_log_files,
            commands::read_log_file,
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
            content::search_projects,
            content::curseforge_status,
            content::get_project,
            content::get_project_members,
            content::get_project_versions,
            content::get_categories,
            content::list_content,
            content::identify_content,
            content::install_content,
            content::check_content_fit,
            servers::list_servers,
            servers::add_server,
            servers::remove_server,
            servers::ping_server,
            servers::recent_servers,
            content::set_content_enabled,
            content::remove_content,
            content::check_content_updates,
            content::install_modpack,
            content::import_pack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bagel Time");
}
