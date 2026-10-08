//! Multiplayer servers: each instance's server list (shared with the game),
//! pinging servers, and the servers played on most recently.

use std::sync::Arc;

use bagel_core::servers::{self, RecentServer, Server, ServerStatus};

use crate::AppState;

type State<'a> = tauri::State<'a, Arc<AppState>>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn game_dir(state: &AppState, id: &str) -> CmdResult<std::path::PathBuf> {
    let instance = state.store.get(id).await.map_err(err)?;
    state.store.game_dir(&instance).map_err(err)
}

#[tauri::command]
pub async fn list_servers(state: State<'_>, id: String) -> CmdResult<Vec<Server>> {
    servers::list(&game_dir(&state, &id).await?).await.map_err(err)
}

/// The game rewrites the list while it runs, so changes wait until it's closed.
fn ensure_stopped(state: &AppState, id: &str) -> CmdResult<()> {
    if state.games.is_running(id) {
        Err("Close the game before changing its server list.".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn add_server(state: State<'_>, id: String, name: String, address: String) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    servers::add(&game_dir(&state, &id).await?, &name, &address)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn remove_server(state: State<'_>, id: String, address: String) -> CmdResult<()> {
    ensure_stopped(&state, &id)?;
    servers::remove(&game_dir(&state, &id).await?, &address)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn ping_server(address: String) -> CmdResult<ServerStatus> {
    servers::ping(&address).await.map_err(err)
}

/// Newest first, only for instances that still exist.
#[tauri::command]
pub async fn recent_servers(state: State<'_>) -> CmdResult<Vec<RecentServer>> {
    let ids: std::collections::HashSet<String> =
        state.store.list().await.map_err(err)?.into_iter().map(|i| i.id).collect();
    Ok(servers::recent(state.launcher.paths())
        .await
        .into_iter()
        .filter(|r| ids.contains(&r.instance_id))
        .collect())
}
