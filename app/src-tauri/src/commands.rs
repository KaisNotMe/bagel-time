//! Functions the frontend calls with `invoke(...)`. Errors are returned as
//! strings so the UI can show them directly.

use std::path::PathBuf;
use std::sync::Arc;

use bagel_core::account::is_valid_username;
use bagel_core::game_files::{self, LogFile, Screenshot, World};
use bagel_core::mrpack::safe_relative_path;
use bagel_core::{GameVersion, Instance, Loader, LoaderVersion, Settings};
use serde::{Deserialize, Serialize};
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
    /// Absolute path of the icon, for the asset protocol.
    icon_path: Option<PathBuf>,
}

pub(crate) fn view(state: &AppState, instance: Instance) -> InstanceView {
    let running = state.games.is_running(&instance.id);
    let icon_path = state.store.icon_path(&instance);
    InstanceView {
        instance,
        running,
        icon_path,
    }
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
pub async fn get_instance(state: State<'_>, id: String) -> CmdResult<InstanceView> {
    let instance = state.store.get(&id).await.map_err(err)?;
    Ok(view(&state, instance))
}

#[tauri::command]
pub async fn list_instances(state: State<'_>) -> CmdResult<Vec<InstanceView>> {
    let instances = state.store.list().await.map_err(err)?;
    Ok(instances.into_iter().map(|i| view(&state, i)).collect())
}

#[tauri::command]
pub async fn list_loader_versions(
    state: State<'_>,
    loader: Loader,
    game_version: String,
) -> CmdResult<Vec<LoaderVersion>> {
    state
        .launcher
        .loader_versions(loader, &game_version)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn create_instance(
    state: State<'_>,
    name: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
) -> CmdResult<InstanceView> {
    let game = resolve_game(&state, game_version, loader, loader_version).await?;
    let instance = state.store.create(&name, game).await.map_err(err)?;
    Ok(view(&state, instance))
}

/// Fills in the newest stable loader version when none was picked.
async fn resolve_game(
    state: &AppState,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
) -> CmdResult<GameVersion> {
    if game_version.trim().is_empty() {
        return Err("Pick a Minecraft version.".into());
    }
    let loader_version = match (loader, loader_version) {
        (Loader::Vanilla, _) => None,
        (_, Some(v)) => Some(v),
        (loader, None) => Some(
            state
                .launcher
                .latest_stable_loader(loader, &game_version)
                .await
                .map_err(err)?
                .ok_or_else(|| format!("{loader} doesn't support Minecraft {game_version} yet."))?,
        ),
    };
    Ok(GameVersion {
        minecraft: game_version,
        loader,
        loader_version,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePatch {
    name: String,
    memory_mb: Option<u32>,
    java_args: Option<String>,
}

/// Saves the settings from the instance settings dialog.
#[tauri::command]
pub async fn update_instance(state: State<'_>, id: String, patch: InstancePatch) -> CmdResult<InstanceView> {
    let mut instance = state.store.get(&id).await.map_err(err)?;
    let name = patch.name.trim();
    if name.is_empty() {
        return Err("Give the instance a name.".into());
    }
    if patch.memory_mb.is_some_and(|m| !(512..=65536).contains(&m)) {
        return Err("Memory must be between 512 MB and 64 GB.".into());
    }
    instance.name = name.chars().take(60).collect();
    instance.memory_mb = patch.memory_mb;
    instance.java_args = patch.java_args.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
    state.store.save(&instance).await.map_err(err)?;
    Ok(view(&state, instance))
}

/// Switches the Minecraft version or loader. Mods may stop working.
#[tauri::command]
pub async fn change_instance_version(
    state: State<'_>,
    id: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
) -> CmdResult<InstanceView> {
    if state.games.is_running(&id) {
        return Err("Close the game before changing its version.".into());
    }
    let mut instance = state.store.get(&id).await.map_err(err)?;
    let game = resolve_game(&state, game_version, loader, loader_version).await?;
    instance.game_version = game.minecraft;
    instance.loader = game.loader;
    instance.loader_version = game.loader_version;
    state.store.save(&instance).await.map_err(err)?;
    Ok(view(&state, instance))
}

#[tauri::command]
pub async fn duplicate_instance(state: State<'_>, id: String, name: String) -> CmdResult<InstanceView> {
    if state.games.is_running(&id) {
        return Err("Close the game before copying this instance.".into());
    }
    let instance = state.store.duplicate(&id, &name).await.map_err(err)?;
    Ok(view(&state, instance))
}

#[tauri::command]
pub async fn set_instance_icon(state: State<'_>, id: String, path: PathBuf) -> CmdResult<InstanceView> {
    let instance = state.store.set_icon_from_file(&id, &path).await.map_err(err)?;
    Ok(view(&state, instance))
}

#[tauri::command]
pub async fn clear_instance_icon(state: State<'_>, id: String) -> CmdResult<InstanceView> {
    let instance = state.store.clear_icon(&id).await.map_err(err)?;
    Ok(view(&state, instance))
}

async fn game_dir(state: &AppState, id: &str) -> CmdResult<PathBuf> {
    let instance = state.store.get(id).await.map_err(err)?;
    state.store.game_dir(&instance).map_err(err)
}

#[tauri::command]
pub async fn list_worlds(state: State<'_>, id: String) -> CmdResult<Vec<World>> {
    game_files::worlds(&game_dir(&state, &id).await?).await.map_err(err)
}

#[tauri::command]
pub async fn list_screenshots(state: State<'_>, id: String) -> CmdResult<Vec<Screenshot>> {
    game_files::screenshots(&game_dir(&state, &id).await?).await.map_err(err)
}

#[tauri::command]
pub async fn delete_screenshot(state: State<'_>, id: String, file_name: String) -> CmdResult<()> {
    game_files::delete_screenshot(&game_dir(&state, &id).await?, &file_name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_log_files(state: State<'_>, id: String) -> CmdResult<Vec<LogFile>> {
    game_files::log_files(&game_dir(&state, &id).await?).await.map_err(err)
}

#[tauri::command]
pub async fn read_log_file(state: State<'_>, id: String, name: String) -> CmdResult<String> {
    game_files::read_log(&game_dir(&state, &id).await?, &name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn delete_instance(state: State<'_>, id: String) -> CmdResult<()> {
    if state.games.is_running(&id) {
        return Err("Close the game before deleting this instance.".into());
    }
    state.store.delete(&id).await.map_err(err)
}

/// Opens the instance's game folder, or a folder inside it (e.g. `mods`,
/// `saves/My World`), in the file manager.
#[tauri::command]
pub async fn open_instance_folder(
    app: AppHandle,
    state: State<'_>,
    id: String,
    sub: Option<String>,
) -> CmdResult<()> {
    let mut dir = game_dir(&state, &id).await?;
    if let Some(sub) = sub {
        dir.push(safe_relative_path(&sub).ok_or("That folder is outside the instance.")?);
    }
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

/// Opens a screenshot in the default image viewer.
#[tauri::command]
pub async fn open_screenshot(app: AppHandle, state: State<'_>, id: String, file_name: String) -> CmdResult<()> {
    let path = safe_relative_path(&file_name)
        .filter(|p| p.components().count() == 1)
        .ok_or("That isn't a screenshot.")?;
    let path = game_dir(&state, &id).await?.join("screenshots").join(path);
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(err)
}
