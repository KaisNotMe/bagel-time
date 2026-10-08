//! playit.gg tunnels, so friends can join a hosted server without port
//! forwarding.
//!
//! Connecting is a one-time step: the launcher makes a claim code, the player
//! approves it on playit.gg (a guest account is fine), and the launcher gets
//! an agent secret back. After that it creates a Minecraft tunnel per server
//! through the playit API and runs the official playit agent (`playitd`,
//! downloaded from playit's GitHub releases) while the server runs.
//!
//! The secret lives in `playit/secret.txt` in the data folder.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::download::{DownloadJob, Downloader};
use crate::error::{Error, IoContext, Result};
use crate::paths::Paths;

const API: &str = "https://api.playit.gg";
const RELEASES: &str = "https://api.github.com/repos/playit-cloud/playit-agent/releases/latest";

#[derive(Clone)]
pub struct Playit {
    dir: PathBuf,
    dl: Downloader,
}

/// Where a claim stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimState {
    /// The player hasn't opened the link yet, or hasn't approved it.
    Waiting,
    /// Approved; the secret is saved.
    Connected,
    Rejected,
}

/// A public address for a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    pub address: String,
}

#[derive(Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "lowercase")]
enum ApiResult<T> {
    Success(T),
    Fail(serde_json::Value),
    Error(serde_json::Value),
}

#[derive(Deserialize)]
struct RunData {
    agent_id: String,
    #[serde(default)]
    tunnels: Vec<AgentTunnel>,
    #[serde(default)]
    pending: Vec<PendingTunnel>,
}

#[derive(Deserialize)]
struct AgentTunnel {
    #[serde(default)]
    name: Option<String>,
    port: PortRange,
    local_port: u16,
    #[serde(default)]
    tunnel_type: Option<String>,
    assigned_domain: String,
    #[serde(default)]
    custom_domain: Option<String>,
    #[serde(default)]
    disabled: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct PortRange {
    from: u16,
}

#[derive(Deserialize)]
struct PendingTunnel {
    #[serde(default)]
    name: Option<String>,
}

impl Playit {
    pub fn new(paths: &Paths, dl: Downloader) -> Self {
        Self { dir: paths.root().join("playit"), dl }
    }

    fn secret_file(&self) -> PathBuf {
        self.dir.join("secret.txt")
    }

    pub async fn secret(&self) -> Option<String> {
        let s = tokio::fs::read_to_string(self.secret_file()).await.ok()?;
        Some(s.trim().to_string()).filter(|s| !s.is_empty())
    }

    pub async fn connected(&self) -> bool {
        self.secret().await.is_some()
    }

    /// Forgets the secret. The agent stays listed on the player's playit account.
    pub async fn disconnect(&self) -> Result<()> {
        match tokio::fs::remove_file(self.secret_file()).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).at(&self.secret_file()),
            _ => Ok(()),
        }
    }

    /// A fresh claim code and the page where the player approves it.
    pub fn new_claim() -> (String, String) {
        let code: String = uuid::Uuid::new_v4().simple().to_string().chars().take(10).collect();
        let url = format!("https://playit.gg/claim/{code}");
        (code, url)
    }

    /// Checks on a claim; once approved, saves the secret.
    pub async fn poll_claim(&self, code: &str) -> Result<ClaimState> {
        if !code.chars().all(|c| c.is_ascii_hexdigit()) || code.is_empty() {
            return Err(Error::Invalid("That isn't a playit claim code.".into()));
        }
        #[derive(Serialize)]
        struct Setup<'a> {
            code: &'a str,
            agent_type: &'a str,
            version: String,
        }
        let state: String = self
            .call(
                "/claim/setup",
                &Setup {
                    code,
                    agent_type: "self-managed",
                    version: format!("Bagel Time {}", env!("CARGO_PKG_VERSION")),
                },
                None,
            )
            .await?;
        match state.as_str() {
            "UserAccepted" => {}
            "UserRejected" => return Ok(ClaimState::Rejected),
            _ => return Ok(ClaimState::Waiting),
        }
        #[derive(Serialize)]
        struct Exchange<'a> {
            code: &'a str,
        }
        #[derive(Deserialize)]
        struct Secret {
            secret_key: String,
        }
        let secret: Secret = match self.call("/claim/exchange", &Exchange { code }, None).await {
            Ok(s) => s,
            // Approved a moment ago; the secret isn't ready yet.
            Err(Error::Invalid(_)) => return Ok(ClaimState::Waiting),
            Err(e) => return Err(e),
        };
        tokio::fs::create_dir_all(&self.dir).await.at(&self.dir)?;
        let path = self.secret_file();
        tokio::fs::write(&path, secret.secret_key.trim()).await.at(&path)?;
        Ok(ClaimState::Connected)
    }

    async fn call<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B, secret: Option<&str>) -> Result<T> {
        let mut req = self.dl.client().post(format!("{API}{path}")).json(body);
        if let Some(secret) = secret {
            req = req.header(reqwest::header::AUTHORIZATION, format!("Agent-Key {secret}"));
        }
        let resp = req.send().await?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::Invalid("playit.gg is busy; try again in a moment.".into()));
        }
        let text = resp.text().await?;
        match serde_json::from_str::<ApiResult<T>>(&text) {
            Ok(ApiResult::Success(v)) => Ok(v),
            Ok(ApiResult::Fail(why)) => Err(Error::Invalid(format!("playit.gg said no: {}", plain(&why)))),
            Ok(ApiResult::Error(why)) => Err(Error::Invalid(format!("playit.gg error: {}", plain(&why)))),
            Err(e) => Err(Error::Invalid(format!("playit.gg sent something unexpected: {e}"))),
        }
    }

    /// The public address for a local server port, creating a Minecraft
    /// tunnel for it if there isn't one. New tunnels can take a little while
    /// to be assigned, so this waits up to a minute.
    pub async fn ensure_tunnel(&self, name: &str, local_port: u16) -> Result<Tunnel> {
        let secret = self
            .secret()
            .await
            .ok_or_else(|| Error::Invalid("Connect playit.gg first.".into()))?;
        let name: String = name.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).take(40).collect();
        let name = format!("Bagel Time: {}", name.trim());
        let mut created = false;
        for _ in 0..30 {
            let data: RunData = self.call("/agents/rundata", &serde_json::json!({}), Some(&secret)).await?;
            let found = data.tunnels.iter().find(|t| {
                t.local_port == local_port
                    && t.disabled.is_none()
                    && t.tunnel_type.as_deref().is_none_or(|k| k == "minecraft-java")
            });
            if let Some(t) = found {
                let host = t.custom_domain.clone().unwrap_or_else(|| t.assigned_domain.clone());
                // Minecraft tunnels get an SRV record, so the port isn't needed.
                let address = if t.tunnel_type.as_deref() == Some("minecraft-java") || t.port.from == 25565 {
                    host
                } else {
                    format!("{host}:{}", t.port.from)
                };
                return Ok(Tunnel { address });
            }
            let pending = data.pending.iter().any(|p| p.name.as_deref() == Some(name.as_str()))
                || data.tunnels.iter().any(|t| t.name.as_deref() == Some(name.as_str()));
            if !created && !pending {
                let body = serde_json::json!({
                    "name": name,
                    "tunnel_type": "minecraft-java",
                    "port_type": "tcp",
                    "port_count": 1,
                    "origin": {
                        "type": "agent",
                        "data": { "agent_id": data.agent_id, "local_ip": "127.0.0.1", "local_port": local_port }
                    },
                    "enabled": true,
                    "alloc": null,
                    "firewall_id": null,
                    "proxy_protocol": null
                });
                let _: serde_json::Value = self.call("/tunnels/create", &body, Some(&secret)).await?;
                created = true;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Err(Error::Invalid(
            "playit.gg hasn't finished setting up the tunnel yet. Try starting the server again in a minute.".into(),
        ))
    }

    fn agent_path(&self) -> PathBuf {
        self.dir.join(if cfg!(windows) { "playitd.exe" } else { "playitd" })
    }

    /// The playit agent, downloaded from playit's GitHub releases the first time.
    pub async fn ensure_agent(&self) -> Result<PathBuf> {
        let path = self.agent_path();
        if path.is_file() {
            return Ok(path);
        }
        let asset = agent_asset().ok_or_else(|| Error::Invalid("playit.gg has no agent for this computer.".into()))?;
        #[derive(Deserialize)]
        struct Release {
            assets: Vec<Asset>,
        }
        #[derive(Deserialize)]
        struct Asset {
            name: String,
            browser_download_url: String,
            size: u64,
        }
        let release: Release = self.dl.get_json(RELEASES).await?;
        let found = release
            .assets
            .iter()
            .find(|a| a.name == asset)
            .or_else(|| release.assets.iter().find(|a| a.name == asset.replace("-signed", "")))
            .ok_or_else(|| Error::Invalid(format!("playit.gg's latest release has no {asset}.")))?;
        self.dl
            .fetch(&DownloadJob {
                url: found.browser_download_url.clone(),
                path: path.clone(),
                sha1: None,
                size: Some(found.size),
            })
            .await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
        Ok(path)
    }

    /// Runs the agent with the saved secret. It has its own control socket
    /// so it doesn't clash with a playit app the player installed.
    pub async fn agent_command(&self) -> Result<tokio::process::Command> {
        let secret = self
            .secret()
            .await
            .ok_or_else(|| Error::Invalid("Connect playit.gg first.".into()))?;
        let exe = self.ensure_agent().await?;
        let socket = if cfg!(windows) {
            r"\\.\pipe\bagel-time-playit".to_string()
        } else {
            self.dir.join("agent.sock").display().to_string()
        };
        let mut cmd = tokio::process::Command::new(&exe);
        cmd.arg("--secret")
            .arg(secret)
            .arg("--socket-path")
            .arg(socket)
            .arg("--log-path")
            .arg(self.dir.join("agent.log"))
            .current_dir(&self.dir)
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        Ok(cmd)
    }

    pub fn log_file(&self) -> PathBuf {
        self.dir.join("agent.log")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// The release file for this platform (the Windows `.exe` is the agent itself).
fn agent_asset() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "playit-windows-x86_64-signed.exe",
        ("windows", "x86") => "playit-windows-x86-signed.exe",
        ("linux", "x86_64") => "playit-linux-amd64",
        ("linux", "aarch64") => "playit-linux-aarch64",
        _ => return None,
    })
}

/// A playit failure value as short text: `"InvalidCode"` or `{"type": ...}`.
fn plain(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string().chars().take(200).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_codes_are_hex() {
        let (code, url) = Playit::new_claim();
        assert_eq!(code.len(), 10);
        assert!(code.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(url, format!("https://playit.gg/claim/{code}"));
    }

    #[test]
    fn parses_api_results() {
        let ok: ApiResult<String> = serde_json::from_str(r#"{"status":"success","data":"WaitingForUserVisit"}"#).unwrap();
        assert!(matches!(ok, ApiResult::Success(s) if s == "WaitingForUserVisit"));
        let fail: ApiResult<String> = serde_json::from_str(r#"{"status":"fail","data":"CodeExpired"}"#).unwrap();
        assert!(matches!(fail, ApiResult::Fail(v) if plain(&v) == "CodeExpired"));
    }
}
