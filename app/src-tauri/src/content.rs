//! Modrinth browsing, content inside instances (mods, resource packs,
//! shaders) and modpack installs.
//!
//! Modpack installs report progress through the `pack-progress` event.

use std::path::PathBuf;
use std::sync::Arc;

use bagel_core::content::{ContentKind, ContentUpdate, InstalledContent, InstanceContent};
use bagel_core::modrinth::{
    Category, ProjectDetails, ProjectType, SearchQuery, SearchResults, SortBy, TeamMember, Version,
};
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
    categories: Vec<String>,
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
        categories: args.categories,
        sort: args.sort,
        offset: args.offset,
        limit: args.limit,
    };
    state.modrinth.search(&query).await.map_err(err)
}

#[tauri::command]
pub async fn get_project(state: State<'_>, id: String) -> CmdResult<ProjectDetails> {
    state.modrinth.project_details(&id).await.map_err(err)
}

#[tauri::command]
pub async fn get_project_members(state: State<'_>, id: String) -> CmdResult<Vec<TeamMember>> {
    state.modrinth.members(&id).await.map_err(err)
}

/// A project's versions, newest first. Empty filters mean "any".
#[tauri::command]
pub async fn get_project_versions(
    state: State<'_>,
    id: String,
    loaders: Vec<String>,
    game_versions: Vec<String>,
) -> CmdResult<Vec<Version>> {
    let loaders: Vec<&str> = loaders.iter().map(String::as_str).collect();
    let game_versions: Vec<&str> = game_versions.iter().map(String::as_str).collect();
    state
        .modrinth
        .project_versions(&id, &loaders, &game_versions)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn get_categories(state: State<'_>) -> CmdResult<Vec<Category>> {
    state.modrinth.categories().await.map_err(err)
}

async fn content(state: &AppState, id: &str, kind: ContentKind) -> CmdResult<InstanceContent> {
    let instance = state.store.get(id).await.map_err(err)?;
    InstanceContent::new(&state.store, &instance, kind).map_err(err)
}

/// Every kind of content the instance can use.
async fn all_content(state: &AppState, id: &str) -> CmdResult<Vec<InstanceContent>> {
    let instance = state.store.get(id).await.map_err(err)?;
    ContentKind::ALL
        .into_iter()
        .filter(|k| k.supported_by(instance.loader))
        .map(|k| InstanceContent::new(&state.store, &instance, k).map_err(err))
        .collect()
}

/// Windows locks jars while the game runs, so changes have to wait.
fn ensure_stopped(state: &AppState, id: &str) -> CmdResult<()> {
    if state.games.is_running(id) {
        Err("Close the game before changing its content.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn list_content(state: State<'_>, id: String) -> CmdResult<Vec<InstalledContent>> {
    let mut out = Vec::new();
    for c in all_content(&state, &id).await? {
        out.extend(c.list().await.map_err(err)?);
    }
    Ok(out)
}

/// Looks up files that were added by hand. Returns true if anything changed.
#[tauri::command]
pub async fn identify_content(state: State<'_>, id: String) -> CmdResult<bool> {
    let mut changed = false;
    for c in all_content(&state, &id).await? {
        changed |= c.identify(&state.modrinth).await.map_err(err)?;
    }
    Ok(changed)
}

#[tauri::command]
pub async fn install_content(
    state: State<'_>,
    id: String,
    kind: ContentKind,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    let c = content(&state, &id, kind).await?;
    let plan = c
        .plan(&state.modrinth, &project_id, version_id.as_deref())
        .await
        .map_err(err)?;
    c.install(&state.modrinth, &plan, &Progress::none())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn set_content_enabled(
    state: State<'_>,
    id: String,
    kind: ContentKind,
    file_name: String,
    enabled: bool,
) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    content(&state, &id, kind)
        .await?
        .set_enabled(&file_name, enabled)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn remove_content(state: State<'_>, id: String, kind: ContentKind, file_name: String) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    content(&state, &id, kind)
        .await?
        .remove(&file_name)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn check_content_updates(state: State<'_>, id: String) -> CmdResult<Vec<ContentUpdate>> {
    let mut out = Vec::new();
    for c in all_content(&state, &id).await? {
        out.extend(c.check_updates(&state.modrinth).await.map_err(err)?);
    }
    Ok(out)
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
