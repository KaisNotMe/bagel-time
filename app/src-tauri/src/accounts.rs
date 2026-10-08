//! Microsoft sign-in commands. Signing in is two-phase: `start_login` returns a
//! code for the player to enter at Microsoft, then a background task waits for
//! them and emits `login-finished` with the account or an error.

use std::sync::Arc;

use bagel_core::accounts::{AccountList, AccountSummary};
use bagel_core::auth::DeviceLogin;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use crate::AppState;

type State<'a> = tauri::State<'a, Arc<AppState>>;

pub struct PendingLogin {
    task: tauri::async_runtime::JoinHandle<()>,
    verification_uri: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginFinished {
    account: Option<AccountSummary>,
    error: Option<String>,
}

#[tauri::command]
pub async fn list_accounts(state: State<'_>) -> Result<AccountList, String> {
    state.accounts.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_login(app: AppHandle, state: State<'_>) -> Result<DeviceLogin, String> {
    cancel(&state);
    let login = state.accounts.start_login().await.map_err(|e| e.to_string())?;

    let task_state = Arc::clone(&state);
    let task_login = login.clone();
    let task = tauri::async_runtime::spawn(async move {
        let result = task_state.accounts.finish_login(&task_login).await;
        task_state.login.lock().unwrap().take();
        let payload = match result {
            Ok(account) => LoginFinished {
                account: Some(account),
                error: None,
            },
            Err(e) => LoginFinished {
                account: None,
                error: Some(e.to_string()),
            },
        };
        let _ = app.emit("login-finished", payload);
    });
    *state.login.lock().unwrap() = Some(PendingLogin {
        task,
        verification_uri: login.verification_uri.clone(),
    });
    Ok(login)
}

/// Opens Microsoft's code entry page. Only the URL Microsoft gave us is opened.
#[tauri::command]
pub fn open_login_page(app: AppHandle, state: State<'_>) -> Result<(), String> {
    let uri = state
        .login
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.verification_uri.clone())
        .ok_or("No sign-in in progress.")?;
    app.opener().open_url(uri, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_login(state: State<'_>) {
    cancel(&state);
}

fn cancel(state: &AppState) {
    if let Some(pending) = state.login.lock().unwrap().take() {
        pending.task.abort();
    }
}

/// `None` switches to offline mode.
#[tauri::command]
pub async fn set_active_account(state: State<'_>, uuid: Option<Uuid>) -> Result<AccountList, String> {
    state.accounts.set_active(uuid).await.map_err(|e| e.to_string())?;
    list_accounts(state).await
}

#[tauri::command]
pub async fn remove_account(state: State<'_>, uuid: Uuid) -> Result<AccountList, String> {
    state.accounts.remove(uuid).await.map_err(|e| e.to_string())?;
    list_accounts(state).await
}
