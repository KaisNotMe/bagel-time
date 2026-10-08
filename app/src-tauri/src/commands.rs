//! Functions the frontend calls with `invoke(...)`. Errors are returned as
//! strings so the UI can show them directly.

use std::sync::Arc;

use bagel_core::account::is_valid_username;
use bagel_core::{Instance, Settings};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::AppState;

type State<'a> = tauri::State<'a, Arc<AppState>>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    id: String,
    kind: String,
    release_time: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    #[serde(flatten)]
    instance: Instance,
    running: bool,
}

fn view(state: &AppState, instance: Instance) -> InstanceView {
    let running = state.games.is_running(&instance.id);
    InstanceView { instance, running }
}

#[tauri::command]
pub async fn list_versions(state: State<'_>, snapshots: bool) -> CmdResult<Vec<VersionInfo>> {
    let manifest = state.launcher.version_manifest().await.map_err(err)?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind == "release" || (snapshots && v.kind == "snapshot"))
        .map(|v| VersionInfo {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect())
}

#[tauri::command]
pub async fn list_instances(state: State<'_>) -> CmdResult<Vec<InstanceView>> {
    let instances = state.store.list().await.map_err(err)?;
    Ok(instances.into_iter().map(|i| view(&state, i)).collect())
}

#[tauri::command]
pub async fn create_instance(state: State<'_>, name: String, game_version: String) -> CmdResult<InstanceView> {
    if game_version.trim().is_empty() {
        return Err("Pick a Minecraft version.".into());
    }
    let instance = state.store.create(&name, &game_version).await.map_err(err)?;
    Ok(view(&state, instance))
}

#[tauri::command]
pub async fn delete_instance(state: State<'_>, id: String) -> CmdResult<()> {
    if state.games.is_running(&id) {
        return Err("Close the game before deleting this instance.".into());
    }
    state.store.delete(&id).await.map_err(err)
}

#[tauri::command]
pub async fn open_instance_folder(app: AppHandle, state: State<'_>, id: String) -> CmdResult<()> {
    let instance = state.store.get(&id).await.map_err(err)?;
    let dir = state.store.game_dir(&instance).map_err(err)?;
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub async fn get_settings(state: State<'_>) -> CmdResult<Settings> {
    Ok(Settings::load(state.launcher.paths()).await)
}

#[tauri::command]
pub async fn save_settings(state: State<'_>, settings: Settings) -> CmdResult<Settings> {
    if !is_valid_username(&settings.offline_username) {
        return Err("Usernames are 3-16 characters: letters, numbers and _ only.".into());
    }
    if !(512..=65536).contains(&settings.memory_mb) {
        return Err("Memory must be between 512 MB and 64 GB.".into());
    }
    settings.save(state.launcher.paths()).await.map_err(err)?;
    Ok(settings)
}

/// Starts installing and launching in the background. Progress, log lines and
/// the exit arrive as events (see `games.rs`).
#[tauri::command]
pub async fn launch_instance(app: AppHandle, state: State<'_>, id: String) -> CmdResult<()> {
    crate::games::launch(app, Arc::clone(&state), id).await
}

#[tauri::command]
pub async fn stop_instance(state: State<'_>, id: String) -> CmdResult<bool> {
    Ok(state.games.stop(&id))
}
