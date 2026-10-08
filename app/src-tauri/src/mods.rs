//! Modrinth browsing, per-instance mods and modpack installs.
//!
//! Modpack installs report progress through the `pack-progress` event.

use std::path::PathBuf;
use std::sync::Arc;

use bagel_core::modrinth::{ProjectType, SearchQuery, SearchResults, SortBy};
use bagel_core::mods::{InstalledMod, InstanceMods, ModUpdate};
use bagel_core::{mrpack, Loader, Progress};
use serde::Deserialize;
use tauri::AppHandle;

use crate::commands::{view, InstanceView};
use crate::games::progress_reporter;
use crate::AppState;

type State<'a> = tauri::State<'a, Arc<AppState>>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchArgs {
    text: String,
    project_type: ProjectType,
    game_version: Option<String>,
    loader: Option<Loader>,
    #[serde(default)]
    sort: SortBy,
    #[serde(default)]
    offset: u32,
    limit: u32,
}

#[tauri::command]
pub async fn search_modrinth(state: State<'_>, args: SearchArgs) -> CmdResult<SearchResults> {
    let query = SearchQuery {
        text: args.text,
        project_type: Some(args.project_type),
        game_version: args.game_version,
        loader: args.loader,
        sort: args.sort,
        offset: args.offset,
        limit: args.limit,
    };
    state.modrinth.search(&query).await.map_err(err)
}

async fn instance_mods(state: &AppState, id: &str) -> CmdResult<InstanceMods> {
    let instance = state.store.get(id).await.map_err(err)?;
    InstanceMods::new(&state.store, &instance).map_err(err)
}

/// Windows locks jars while the game runs, so changes have to wait.
fn ensure_stopped(state: &AppState, id: &str) -> CmdResult<()> {
    if state.games.is_running(id) {
        Err("Close the game before changing mods.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn list_mods(state: State<'_>, id: String) -> CmdResult<Vec<InstalledMod>> {
    instance_mods(&state, &id).await?.list().await.map_err(err)
}

/// Looks up mods that were added by hand. Returns true if the list changed.
#[tauri::command]
pub async fn identify_mods(state: State<'_>, id: String) -> CmdResult<bool> {
    instance_mods(&state, &id)
        .await?
        .identify(&state.modrinth)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn install_mod(
    state: State<'_>,
    id: String,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<Vec<InstalledMod>> {
    ensure_stopped(&state, &id)?;
    let instance = state.store.get(&id).await.map_err(err)?;
    let mods = InstanceMods::new(&state.store, &instance).map_err(err)?;
    let installed = mods.installed_projects().await.map_err(err)?;
    let plan = state
        .modrinth
        .plan_install(&project_id, version_id.as_deref(), &instance.game(), &installed)
        .await
        .map_err(err)?;
    mods.install(&state.modrinth, &plan, &Progress::none())
        .await
        .map_err(err)?;
    mods.list().await.map_err(err)
}

#[tauri::command]
pub async fn set_mod_enabled(state: State<'_>, id: String, file_name: String, enabled: bool) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    instance_mods(&state, &id)
        .await?
        .set_enabled(&file_name, enabled)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn remove_mod(state: State<'_>, id: String, file_name: String) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    instance_mods(&state, &id)
        .await?
        .remove(&file_name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn check_mod_updates(state: State<'_>, id: String) -> CmdResult<Vec<ModUpdate>> {
    instance_mods(&state, &id)
        .await?
        .check_updates(&state.modrinth)
        .await
        .map_err(err)
}

fn pack_progress(app: &AppHandle) -> Progress {
    progress_reporter(app.clone(), "pack-progress", String::new())
}

/// Installs a Modrinth modpack as a new instance.
#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_>,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<InstanceView> {
    let instance = mrpack::install_from_modrinth(
        state.launcher.paths(),
        &state.store,
        &state.modrinth,
        &project_id,
        version_id.as_deref(),
        &pack_progress(&app),
    )
    .await
    .map_err(err)?;
    Ok(view(&state, instance))
}

/// Creates an instance from a `.mrpack` file on disk.
#[tauri::command]
pub async fn import_mrpack(app: AppHandle, state: State<'_>, path: PathBuf) -> CmdResult<InstanceView> {
    let instance = mrpack::install_pack(&state.store, &state.modrinth, &path, &pack_progress(&app))
        .await
        .map_err(err)?;
    Ok(view(&state, instance))
}
