//! Running games: launching in the background, streaming their logs to the
//! UI, and stopping them.
//!
//! Events sent to the frontend:
//! - `launch-progress` while files download
//! - `game-started` once Java is running
//! - `game-log` for each log line
//! - `game-exited` when the game closes or the launch failed

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use bagel_core::logs::{LogLine, LogParser};
use bagel_core::{Account, Instance, LaunchOptions, Progress, ProgressEvent, Settings};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::oneshot;

use crate::AppState;

/// Instance id -> a way to stop it (none yet while still downloading).
#[derive(Default)]
pub struct Games {
    running: Mutex<HashMap<String, Option<oneshot::Sender<()>>>>,
}

impl Games {
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }

    /// Returns false if the instance is already starting or running.
    fn claim(&self, id: &str) -> bool {
        let mut running = self.running.lock().unwrap();
        if running.contains_key(id) {
            return false;
        }
        running.insert(id.to_string(), None);
        true
    }

    fn set_stopper(&self, id: &str, tx: oneshot::Sender<()>) {
        if let Some(slot) = self.running.lock().unwrap().get_mut(id) {
            *slot = Some(tx);
        }
    }

    fn release(&self, id: &str) {
        self.running.lock().unwrap().remove(id);
    }

    pub fn stop(&self, id: &str) -> bool {
        let tx = self
            .running
            .lock()
            .unwrap()
            .get_mut(id)
            .and_then(Option::take);
        tx.is_some_and(|tx| tx.send(()).is_ok())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    instance_id: String,
    stage: String,
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstancePayload {
    instance_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogPayload {
    instance_id: String,
    line: LogLine,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExitPayload {
    instance_id: String,
    code: Option<i32>,
    error: Option<String>,
}

pub async fn launch(app: AppHandle, state: Arc<AppState>, id: String) -> Result<(), String> {
    let instance = state.store.get(&id).await.map_err(|e| e.to_string())?;
    if !state.games.claim(&id) {
        return Err("This instance is already running.".into());
    }
    tauri::async_runtime::spawn(async move {
        let result = run_game(&app, &state, &instance).await;
        state.games.release(&id);
        let (code, error) = match result {
            Ok(code) => (code, None),
            Err(e) => (None, Some(e)),
        };
        let _ = app.emit(
            "game-exited",
            ExitPayload {
                instance_id: id,
                code,
                error,
            },
        );
    });
    Ok(())
}

async fn run_game(app: &AppHandle, state: &AppState, instance: &Instance) -> Result<Option<i32>, String> {
    let id = instance.id.clone();
    let settings = Settings::load(state.launcher.paths()).await;
    let options = LaunchOptions {
        account: Account::offline(&settings.offline_username),
        game_dir: state.store.game_dir(instance).map_err(|e| e.to_string())?,
        memory_mb: instance.memory_mb.unwrap_or(settings.memory_mb),
    };

    let progress = progress_reporter(app.clone(), id.clone());
    let mut cmd = state
        .launcher
        .prepare_launch(&instance.game(), &options, &progress)
        .await
        .map_err(|e| e.to_string())?;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start Java: {e}"))?;

    let _ = state.store.mark_played(&id).await;
    let (stop_tx, stop_rx) = oneshot::channel();
    state.games.set_stopper(&id, stop_tx);
    let _ = app.emit("game-started", InstancePayload { instance_id: id.clone() });

    // The pipes must be drained continuously or the game blocks on writing.
    let stdout = tokio::spawn(pipe_logs(app.clone(), id.clone(), child.stdout.take()));
    let stderr = tokio::spawn(pipe_logs(app.clone(), id.clone(), child.stderr.take()));

    let status = tokio::select! {
        status = child.wait() => status,
        Ok(()) = stop_rx => {
            let _ = child.kill().await;
            child.wait().await
        }
    }
    .map_err(|e| e.to_string())?;
    let _ = stdout.await;
    let _ = stderr.await;
    Ok(status.code())
}

async fn pipe_logs<R: AsyncRead + Unpin>(app: AppHandle, id: String, reader: Option<R>) {
    let Some(reader) = reader else { return };
    let mut reader = BufReader::new(reader);
    let mut parser = LogParser::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                // Lossy so one odd byte doesn't stop the log.
                let text = String::from_utf8_lossy(&buf);
                if let Some(line) = parser.feed(text.trim_end_matches(['\r', '\n'])) {
                    let _ = app.emit(
                        "game-log",
                        LogPayload {
                            instance_id: id.clone(),
                            line,
                        },
                    );
                }
            }
        }
    }
}

/// Forwards progress to the UI, at most ~100 updates per stage.
fn progress_reporter(app: AppHandle, id: String) -> Progress {
    let current = Mutex::new((String::new(), 0u64, 0u64)); // stage, total, done
    Progress::new(move |event| {
        let mut cur = current.lock().unwrap();
        match event {
            ProgressEvent::Stage { name, total } => *cur = (name, total, 0),
            ProgressEvent::Advance(n) => {
                cur.2 += n;
                let step = (cur.1 / 100).max(1);
                if cur.2 % step != 0 && cur.2 < cur.1 {
                    return;
                }
            }
        }
        let _ = app.emit(
            "launch-progress",
            ProgressPayload {
                instance_id: id.clone(),
                stage: cur.0.clone(),
                done: cur.2,
                total: cur.1,
            },
        );
    })
}
