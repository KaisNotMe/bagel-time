//! Servers the player hosts: creating them, running them with a live console,
//! and sharing them through a playit.gg tunnel.
//!
//! Events sent to the frontend:
//! - `host-status` whenever a server's state, players or address change
//! - `host-log` for each console line

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bagel_core::hosting::{self, Created, HostedServer, NewServer, ServerEvent};
use bagel_core::logs::{LogLine, LogParser};
use bagel_core::playit::ClaimState;
use bagel_core::{GameVersion, Loader, Progress, ProgressEvent};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::AppState;

type State<'a> = tauri::State<'a, Arc<AppState>>;
type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Console lines kept per server for when the page is reopened.
const MAX_LINES: usize = 2000;
/// How long a server gets to save and stop before it's killed.
const STOP_GRACE: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Stopped,
    /// Downloading, installing or booting.
    Starting,
    Running,
    Stopping,
}

/// What the UI shows about a running (or just stopped) server.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    phase: Phase,
    /// What's happening while starting, e.g. "Installing NeoForge".
    stage: String,
    players: Vec<String>,
    /// The tunnel address, once the tunnel is up.
    public_address: Option<String>,
    /// Why the tunnel isn't working, if it isn't.
    tunnel_error: Option<String>,
    /// Why the server stopped, if it crashed or failed to start.
    error: Option<String>,
}

impl Status {
    fn stopped() -> Self {
        Status {
            phase: Phase::Stopped,
            stage: String::new(),
            players: Vec::new(),
            public_address: None,
            tunnel_error: None,
            error: None,
        }
    }
}

struct Running {
    status: Status,
    lines: VecDeque<LogLine>,
    /// Console input; `None` until the server process is up.
    input: Option<mpsc::UnboundedSender<String>>,
    stop_requested: bool,
}

/// Every server that's running or was this session.
#[derive(Default)]
pub struct Hosting {
    servers: Mutex<HashMap<String, Running>>,
    agent: Mutex<Agent>,
}

/// The playit agent, shared by every server with a tunnel. It runs while
/// anything holds an [`AgentUse`].
#[derive(Default)]
struct Agent {
    child: Option<tokio::process::Child>,
    users: usize,
}

/// Keeps the playit agent running until dropped.
pub struct AgentUse(Arc<AppState>);

impl Drop for AgentUse {
    fn drop(&mut self) {
        let mut agent = self.0.hosting.agent.lock().unwrap();
        agent.users = agent.users.saturating_sub(1);
        if agent.users == 0 {
            if let Some(mut child) = agent.child.take() {
                let _ = child.start_kill();
            }
        }
    }
}

/// Starts the playit agent if it isn't running, and keeps it running while
/// the returned guard lives.
async fn agent_acquire(state: &Arc<AppState>) -> Result<AgentUse, String> {
    let mut cmd = state.playit.agent_command().await.map_err(err)?;
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut agent = state.hosting.agent.lock().unwrap();
    let alive = agent.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
    if !alive {
        agent.child = Some(cmd.spawn().map_err(|e| format!("Couldn't start the playit agent: {e}"))?);
    }
    agent.users += 1;
    Ok(AgentUse(Arc::clone(state)))
}

impl Hosting {
    pub fn is_active(&self, id: &str) -> bool {
        self.servers
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|r| r.status.phase != Phase::Stopped)
    }

    fn status(&self, id: &str) -> Status {
        self.servers
            .lock()
            .unwrap()
            .get(id)
            .map_or_else(Status::stopped, |r| r.status.clone())
    }

    /// Returns false if it's already starting or running.
    fn claim(&self, id: &str) -> bool {
        let mut servers = self.servers.lock().unwrap();
        if servers.get(id).is_some_and(|r| r.status.phase != Phase::Stopped) {
            return false;
        }
        servers.insert(
            id.to_string(),
            Running {
                status: Status { phase: Phase::Starting, stage: "Starting".into(), ..Status::stopped() },
                lines: VecDeque::new(),
                input: None,
                stop_requested: false,
            },
        );
        true
    }

    fn update(&self, app: &AppHandle, id: &str, f: impl FnOnce(&mut Status)) {
        let status = {
            let mut servers = self.servers.lock().unwrap();
            let Some(r) = servers.get_mut(id) else { return };
            f(&mut r.status);
            r.status.clone()
        };
        let _ = app.emit("host-status", StatusPayload { server_id: id.to_string(), status });
    }

    fn push_line(&self, app: &AppHandle, id: &str, line: LogLine) {
        if let Some(r) = self.servers.lock().unwrap().get_mut(id) {
            if r.lines.len() >= MAX_LINES {
                r.lines.pop_front();
            }
            r.lines.push_back(line.clone());
        }
        let _ = app.emit("host-log", LogPayload { server_id: id.to_string(), line });
    }

    fn send(&self, id: &str, command: String) -> bool {
        self.servers
            .lock()
            .unwrap()
            .get(id)
            .and_then(|r| r.input.clone())
            .is_some_and(|tx| tx.send(command).is_ok())
    }

    /// Asks every running server to save and stop (used when the app closes).
    pub fn stop_all(&self) -> usize {
        let mut servers = self.servers.lock().unwrap();
        let mut n = 0;
        for r in servers.values_mut() {
            if r.status.phase == Phase::Stopped {
                continue;
            }
            if let Some(tx) = &r.input {
                r.stop_requested = true;
                let _ = tx.send("stop".into());
                n += 1;
            }
        }
        n
    }

    pub fn any_active(&self) -> bool {
        self.servers.lock().unwrap().values().any(|r| r.status.phase != Phase::Stopped)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusPayload {
    server_id: String,
    status: Status,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogPayload {
    server_id: String,
    line: LogLine,
}

/// A hosted server with its live status.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedView {
    #[serde(flatten)]
    server: HostedServer,
    status: Status,
}

fn view(state: &AppState, server: HostedServer) -> HostedView {
    let mut status = state.hosting.status(&server.id);
    if status.public_address.is_none() && server.tunnel {
        status.public_address = server.public_address.clone();
    }
    HostedView { status, server }
}

#[tauri::command]
pub async fn list_hosted_servers(state: State<'_>) -> CmdResult<Vec<HostedView>> {
    let list = state.hosts.list().await.map_err(err)?;
    Ok(list.into_iter().map(|s| view(&state, s)).collect())
}

#[tauri::command]
pub async fn get_hosted_server(state: State<'_>, id: String) -> CmdResult<HostedView> {
    let server = state.hosts.get(&id).await.map_err(err)?;
    Ok(view(&state, server))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArgs {
    name: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
    online_mode: bool,
    tunnel: bool,
    accept_eula: bool,
    from_instance: Option<String>,
}

#[tauri::command]
pub async fn create_hosted_server(state: State<'_>, args: CreateArgs) -> CmdResult<Created> {
    let new = NewServer {
        name: args.name,
        game: GameVersion {
            minecraft: args.game_version,
            loader: args.loader,
            loader_version: args.loader_version,
        },
        online_mode: args.online_mode,
        tunnel: args.tunnel,
        accept_eula: args.accept_eula,
        from_instance: args.from_instance,
    };
    state.hosts.create(new, &state.store, &state.sources).await.map_err(err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPatch {
    name: String,
    memory_mb: u32,
    online_mode: bool,
    whitelist: bool,
    tunnel: bool,
}

#[tauri::command]
pub async fn update_hosted_server(state: State<'_>, id: String, patch: ServerPatch) -> CmdResult<HostedView> {
    if state.hosting.is_active(&id) {
        return Err("Stop the server before changing its settings.".into());
    }
    let mut server = state.hosts.get(&id).await.map_err(err)?;
    if !patch.name.trim().is_empty() {
        server.name = patch.name.trim().to_string();
    }
    server.memory_mb = patch.memory_mb.clamp(1024, 65536);
    server.online_mode = patch.online_mode;
    server.whitelist = patch.whitelist;
    server.tunnel = patch.tunnel;
    state.hosts.update(&server).await.map_err(err)?;
    Ok(view(&state, server))
}

#[tauri::command]
pub async fn delete_hosted_server(state: State<'_>, id: String) -> CmdResult<()> {
    if state.hosting.is_active(&id) {
        return Err("Stop the server before deleting it.".into());
    }
    state.hosts.delete(&id).await.map_err(err)
}

#[tauri::command]
pub async fn open_hosted_folder(app: AppHandle, state: State<'_>, id: String) -> CmdResult<()> {
    let dir = state.hosts.server_dir(&id).map_err(err)?;
    tokio::fs::create_dir_all(&dir).await.map_err(err)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(err)
}

/// Console lines so far (for a page opened after the server started).
#[tauri::command]
pub fn server_console(state: State<'_>, id: String) -> Vec<LogLine> {
    state
        .hosting
        .servers
        .lock()
        .unwrap()
        .get(&id)
        .map(|r| r.lines.iter().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn send_server_command(state: State<'_>, id: String, command: String) -> CmdResult<()> {
    let command = command.trim().trim_start_matches('/').to_string();
    if command.is_empty() {
        return Ok(());
    }
    if state.hosting.send(&id, command) {
        Ok(())
    } else {
        Err("The server isn't running.".into())
    }
}

#[tauri::command]
pub fn stop_hosted_server(app: AppHandle, state: State<'_>, id: String) -> CmdResult<()> {
    {
        let mut servers = state.hosting.servers.lock().unwrap();
        let Some(r) = servers.get_mut(&id) else { return Ok(()) };
        if r.status.phase == Phase::Stopped {
            return Ok(());
        }
        r.stop_requested = true;
    }
    // Still downloading: nothing to send; the start task notices `stop_requested`.
    let sent = state.hosting.send(&id, "stop".into());
    state.hosting.update(&app, &id, |s| {
        s.phase = Phase::Stopping;
        s.stage = if sent { "Saving the world".into() } else { "Stopping".into() };
    });
    Ok(())
}

#[tauri::command]
pub async fn start_hosted_server(app: AppHandle, state: State<'_>, id: String) -> CmdResult<()> {
    let server = state.hosts.get(&id).await.map_err(err)?;
    if !state.hosting.claim(&id) {
        return Err("This server is already running.".into());
    }
    state.hosting.update(&app, &id, |_| {});
    let state = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let result = run_server(&app, &state, server).await;
        state.hosting.update(&app, &id, |s| {
            s.phase = Phase::Stopped;
            s.stage = String::new();
            s.players.clear();
            if let Err(e) = result {
                s.error = Some(e);
            }
        });
    });
    Ok(())
}

async fn run_server(app: &AppHandle, state: &Arc<AppState>, server: HostedServer) -> Result<(), String> {
    let id = server.id.clone();
    let hosting = &state.hosting;
    let stage = |text: &str| {
        let text = text.to_string();
        hosting.update(app, &id, move |s| s.stage = text);
    };
    let stop_requested = || hosting.servers.lock().unwrap().get(&id).is_some_and(|r| r.stop_requested);

    // The tunnel first, so its address is ready by the time the server is.
    let mut _agent = None;
    if server.tunnel {
        if state.playit.connected().await {
            stage("Opening the tunnel");
            match start_tunnel(state, &server).await {
                Ok((address, guard)) => {
                    _agent = Some(guard);
                    let mut saved = server.clone();
                    saved.public_address = Some(address.clone());
                    let _ = state.hosts.save(&saved).await;
                    hosting.update(app, &id, |s| {
                        s.public_address = Some(address);
                        s.tunnel_error = None;
                    });
                }
                Err(e) => hosting.update(app, &id, |s| s.tunnel_error = Some(e)),
            }
        } else {
            hosting.update(app, &id, |s| {
                s.tunnel_error = Some("Connect playit.gg so friends can join over the internet.".into())
            });
        }
    }

    let dir = state.hosts.server_dir(&id).map_err(err)?;
    let progress = stage_reporter(app.clone(), Arc::clone(state), id.clone());
    let mut cmd = state
        .launcher
        .prepare_server(&server.game(), &dir, server.memory_mb, &progress)
        .await
        .map_err(err)?;
    if stop_requested() {
        return Ok(());
    }
    stage("Starting the server");
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't start Java: {e}"))?;
    let _ = state.hosts.mark_started(&id).await;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    if let Some(r) = hosting.servers.lock().unwrap().get_mut(&id) {
        r.input = Some(tx);
    }
    let mut stdin = child.stdin.take();
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let Some(pipe) = stdin.as_mut() else { break };
            if pipe.write_all(format!("{line}\n").as_bytes()).await.is_err() || pipe.flush().await.is_err() {
                break;
            }
        }
    });
    let out = tokio::spawn(pipe_console(app.clone(), Arc::clone(state), id.clone(), child.stdout.take()));
    let errs = tokio::spawn(pipe_console(app.clone(), Arc::clone(state), id.clone(), child.stderr.take()));

    // Wait for it to exit; once a stop is asked for, give it time to save.
    let status = loop {
        tokio::select! {
            status = child.wait() => break status,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                if stop_requested() {
                    match tokio::time::timeout(STOP_GRACE, child.wait()).await {
                        Ok(status) => break status,
                        Err(_) => {
                            let _ = child.kill().await;
                            break child.wait().await;
                        }
                    }
                }
            }
        }
    };
    writer.abort();
    let _ = out.await;
    let _ = errs.await;
    if let Some(r) = hosting.servers.lock().unwrap().get_mut(&id) {
        r.input = None;
    }
    let status = status.map_err(err)?;
    if !status.success() && !stop_requested() {
        return Err(format!(
            "The server stopped unexpectedly (exit code {}). Check the console for the reason.",
            status.code().map_or("unknown".into(), |c| c.to_string())
        ));
    }
    Ok(())
}

/// Makes sure the tunnel exists and the playit agent is running.
async fn start_tunnel(state: &Arc<AppState>, server: &HostedServer) -> Result<(String, AgentUse), String> {
    // The agent goes first: playit only makes tunnels for agents it has
    // heard from.
    let guard = agent_acquire(state).await?;
    let tunnel = state.playit.ensure_tunnel(&server.name, server.port).await.map_err(err)?;
    Ok((tunnel.address, guard))
}

async fn pipe_console<R: AsyncRead + Unpin>(app: AppHandle, state: Arc<AppState>, id: String, reader: Option<R>) {
    let Some(reader) = reader else { return };
    let mut reader = BufReader::new(reader);
    let mut parser = LogParser::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                let Some(line) = parser.feed(text.trim_end_matches(['\r', '\n'])) else { continue };
                match hosting::server_event(&line.message) {
                    Some(ServerEvent::Ready) => state.hosting.update(&app, &id, |s| {
                        s.phase = Phase::Running;
                        s.stage = String::new();
                    }),
                    Some(ServerEvent::Joined(name)) => state.hosting.update(&app, &id, |s| {
                        if !s.players.contains(&name) {
                            s.players.push(name);
                        }
                    }),
                    Some(ServerEvent::Left(name)) => state.hosting.update(&app, &id, |s| s.players.retain(|p| p != &name)),
                    None => {}
                }
                state.hosting.push_line(&app, &id, line);
            }
        }
    }
}

/// Download and install progress as the server's stage text.
fn stage_reporter(app: AppHandle, state: Arc<AppState>, id: String) -> Progress {
    let current = Mutex::new((String::new(), 0u64, 0u64));
    Progress::new(move |event| {
        let mut cur = current.lock().unwrap();
        match event {
            ProgressEvent::Stage { name, total } => *cur = (name, total, 0),
            ProgressEvent::Advance(n) => {
                cur.2 += n;
                let step = (cur.1 / 50).max(1);
                if cur.2 % step != 0 && cur.2 < cur.1 {
                    return;
                }
            }
        }
        let text = if cur.1 > 1 {
            format!("{} {}%", cur.0, cur.2 * 100 / cur.1.max(1))
        } else {
            cur.0.clone()
        };
        state.hosting.update(&app, &id, |s| s.stage = text);
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayitStatus {
    connected: bool,
}

#[tauri::command]
pub async fn playit_status(state: State<'_>) -> CmdResult<PlayitStatus> {
    Ok(PlayitStatus { connected: state.playit.connected().await })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayitClaim {
    code: String,
    url: String,
}

/// A link for the player to approve Bagel Time on playit.gg.
#[tauri::command]
pub fn playit_start_claim() -> PlayitClaim {
    let (code, url) = bagel_core::playit::Playit::new_claim();
    PlayitClaim { code, url }
}

#[tauri::command]
pub async fn playit_poll_claim(state: State<'_>, code: String) -> CmdResult<ClaimState> {
    let result = state.playit.poll_claim(&code).await.map_err(err)?;
    if result == ClaimState::Connected {
        // playit.gg's setup page waits until it sees the agent online, so run
        // it for a few minutes even if no server is starting.
        if let Ok(guard) = agent_acquire(&state).await {
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(180)).await;
                drop(guard);
            });
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn playit_disconnect(state: State<'_>) -> CmdResult<()> {
    if state.hosting.any_active() {
        return Err("Stop your servers before disconnecting playit.gg.".into());
    }
    state.playit.disconnect().await.map_err(err)
}
