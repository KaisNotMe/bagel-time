//! Browsing Modrinth and CurseForge, content inside instances (mods,
//! resource packs, shaders) and modpack installs.
//!
//! Every browsing command takes a `source`; results have the same shape for
//! both sites. Modpack installs report progress through the `pack-progress`
//! event.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bagel_core::cfpack::{self, ManualDownload, PackInstall};
use bagel_core::content::{check_fit, ContentKind, ContentUpdate, Fit, InstalledContent, InstanceContent, Source};
use bagel_core::curseforge::CurseForge;
use bagel_core::modrinth::{
    Category, ProjectDetails, ProjectType, SearchQuery, SearchResults, SortBy, TeamMember, Version,
};
use bagel_core::{mrpack, Loader, Progress};
use serde::{Deserialize, Serialize};
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
    #[serde(default)]
    source: Source,
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
pub async fn search_projects(state: State<'_>, args: SearchArgs) -> CmdResult<SearchResults> {
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
    match args.source {
        Source::Modrinth => state.sources.modrinth.search(&query).await,
        Source::CurseForge => state.sources.curseforge.search(&query).await,
    }
    .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeStatus {
    /// A key is available from somewhere.
    has_key: bool,
    /// The key comes from the environment or the build, not settings.
    managed: bool,
}

#[tauri::command]
pub fn curseforge_status(state: State<'_>) -> CurseForgeStatus {
    CurseForgeStatus {
        has_key: state.sources.curseforge.has_key(),
        managed: CurseForge::key_from_env() || CurseForge::has_builtin_key(),
    }
}

/// Asks CurseForge whether the current key works.
#[tauri::command]
pub async fn check_curseforge_key(state: State<'_>) -> CmdResult<()> {
    state.sources.curseforge.check_key().await.map_err(err)
}

#[tauri::command]
pub async fn get_project(state: State<'_>, source: Source, id: String) -> CmdResult<ProjectDetails> {
    match source {
        Source::Modrinth => state.sources.modrinth.project_details(&id).await,
        Source::CurseForge => state.sources.curseforge.project_details(&id).await,
    }
    .map_err(err)
}

#[tauri::command]
pub async fn get_project_members(state: State<'_>, source: Source, id: String) -> CmdResult<Vec<TeamMember>> {
    match source {
        Source::Modrinth => state.sources.modrinth.members(&id).await,
        Source::CurseForge => state.sources.curseforge.members(&id).await,
    }
    .map_err(err)
}

/// A project's versions, newest first. Empty filters mean "any".
#[tauri::command]
pub async fn get_project_versions(
    state: State<'_>,
    source: Source,
    id: String,
    loaders: Vec<String>,
    game_versions: Vec<String>,
) -> CmdResult<Vec<Version>> {
    let loaders: Vec<&str> = loaders.iter().map(String::as_str).collect();
    let game_versions: Vec<&str> = game_versions.iter().map(String::as_str).collect();
    match source {
        Source::Modrinth => {
            state
                .sources
                .modrinth
                .project_versions(&id, &loaders, &game_versions)
                .await
        }
        Source::CurseForge => {
            state
                .sources
                .curseforge
                .project_versions(&id, &loaders, &game_versions)
                .await
        }
    }
    .map_err(err)
}

#[tauri::command]
pub async fn get_categories(state: State<'_>, source: Source) -> CmdResult<Vec<Category>> {
    match source {
        Source::Modrinth => state.sources.modrinth.categories().await,
        Source::CurseForge => state.sources.curseforge.categories().await,
    }
    .map_err(err)
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
        changed |= c.identify(&state.sources).await.map_err(err)?;
    }
    Ok(changed)
}

#[tauri::command]
pub async fn install_content(
    state: State<'_>,
    id: String,
    kind: ContentKind,
    source: Source,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    let c = content(&state, &id, kind).await?;
    let plan = c
        .plan(&state.sources, source, &project_id, version_id.as_deref())
        .await
        .map_err(err)?;
    c.install(state.sources.modrinth.downloader(), &plan, &Progress::none())
        .await
        .map_err(err)
}

/// For each instance: can this project (or this version of it) go in, and
/// which version would. Keyed by instance id.
#[tauri::command]
pub async fn check_content_fit(
    state: State<'_>,
    kind: ContentKind,
    source: Source,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<HashMap<String, Fit>> {
    let list = state.store.list().await.map_err(err)?;
    let games: Vec<_> = list.iter().map(|i| i.game()).collect();
    let fits = check_fit(&state.sources, source, kind, &project_id, version_id.as_deref(), &games)
        .await
        .map_err(err)?;
    Ok(list.into_iter().map(|i| i.id).zip(fits).collect())
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
        out.extend(c.check_updates(&state.sources).await.map_err(err)?);
    }
    Ok(out)
}

fn pack_progress(app: &AppHandle) -> Progress {
    progress_reporter(app.clone(), "pack-progress", String::new())
}

/// A new instance from a modpack, plus files to download by hand.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackResult {
    instance: InstanceView,
    manual: Vec<ManualDownload>,
}

fn pack_result(state: &AppState, done: PackInstall) -> PackResult {
    PackResult {
        instance: view(state, done.instance),
        manual: done.manual,
    }
}

/// Installs a modpack from Modrinth or CurseForge as a new instance.
#[tauri::command]
pub async fn install_modpack(
    app: AppHandle,
    state: State<'_>,
    source: Source,
    project_id: String,
    version_id: Option<String>,
) -> CmdResult<PackResult> {
    let paths = state.launcher.paths();
    let progress = pack_progress(&app);
    let done = match source {
        Source::Modrinth => PackInstall {
            instance: mrpack::install_from_modrinth(
                paths,
                &state.store,
                &state.sources,
                &project_id,
                version_id.as_deref(),
                &progress,
            )
            .await
            .map_err(err)?,
            manual: Vec::new(),
        },
        Source::CurseForge => cfpack::install_from_curseforge(
            paths,
            &state.store,
            &state.sources,
            &project_id,
            version_id.as_deref(),
            &progress,
        )
        .await
        .map_err(err)?,
    };
    Ok(pack_result(&state, done))
}

/// Creates an instance from a `.mrpack` or CurseForge `.zip` on disk.
#[tauri::command]
pub async fn import_pack(app: AppHandle, state: State<'_>, path: PathBuf) -> CmdResult<PackResult> {
    let done = cfpack::import_file(&state.store, &state.sources, &path, &pack_progress(&app))
        .await
        .map_err(err)?;
    Ok(pack_result(&state, done))
}
