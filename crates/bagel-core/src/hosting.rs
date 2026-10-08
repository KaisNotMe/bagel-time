//! Servers the player hosts on their own computer.
//!
//! Layout: `hosted/<id>/server.json` plus `hosted/<id>/server/` (the server's
//! own folder: world, mods, `server.properties`). Installing the server files
//! and building the command to run them lives in [`crate::Launcher::prepare_server`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::content::{ContentKind, InstanceContent, Source, Sources};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::instance::{Instance, InstanceStore, copy_dir, is_valid_id, now, slugify};
use crate::loaders::{GameVersion, Loader};
use crate::paths::Paths;

const META_FILE: &str = "server.json";
pub const DEFAULT_PORT: u16 = 25565;
pub const DEFAULT_MEMORY_MB: u32 = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedServer {
    /// Folder name; also the stable identifier the UI uses.
    pub id: String,
    pub name: String,
    pub game_version: String,
    #[serde(default)]
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub memory_mb: u32,
    pub port: u16,
    /// Only real Minecraft accounts may join. Off lets offline players in.
    pub online_mode: bool,
    /// Only players on the whitelist may join.
    #[serde(default)]
    pub whitelist: bool,
    /// Share it over the internet through a playit.gg tunnel.
    pub tunnel: bool,
    /// The tunnel's public address, once known.
    #[serde(default)]
    pub public_address: Option<String>,
    /// The instance it was made from, if any.
    #[serde(default)]
    pub from_instance: Option<String>,
    /// Unix seconds.
    pub created: u64,
    #[serde(default)]
    pub last_started: Option<u64>,
}

impl HostedServer {
    pub fn game(&self) -> GameVersion {
        GameVersion {
            minecraft: self.game_version.clone(),
            loader: self.loader,
            loader_version: self.loader_version.clone(),
        }
    }
}

/// What to make a new server from.
#[derive(Debug, Clone)]
pub struct NewServer {
    pub name: String,
    pub game: GameVersion,
    pub online_mode: bool,
    pub tunnel: bool,
    /// The player agreed to the Minecraft EULA (required to run a server).
    pub accept_eula: bool,
    /// Copy mods and their configs from this instance.
    pub from_instance: Option<String>,
}

/// A newly made server, plus mods that were left out because they only
/// work in the game client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub server: HostedServer,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HostStore {
    dir: PathBuf,
}

impl HostStore {
    pub fn new(paths: &Paths) -> Self {
        Self { dir: paths.root().join("hosted") }
    }

    pub fn server_meta_dir(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_id(id) {
            return Err(Error::Invalid(format!("No server called \"{id}\".")));
        }
        Ok(self.dir.join(id))
    }

    /// The server's own folder (world, mods, server.properties).
    pub fn server_dir(&self, id: &str) -> Result<PathBuf> {
        Ok(self.server_meta_dir(id)?.join("server"))
    }

    /// All hosted servers, most recently started first.
    pub async fn list(&self) -> Result<Vec<HostedServer>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).at(&self.dir),
        };
        while let Some(entry) = entries.next_entry().await.at(&self.dir)? {
            let path = entry.path().join(META_FILE);
            if let Ok(bytes) = tokio::fs::read(&path).await
                && let Ok(server) = serde_json::from_slice::<HostedServer>(&bytes)
            {
                out.push(server);
            }
        }
        out.sort_by_key(|s| std::cmp::Reverse(s.last_started.unwrap_or(s.created)));
        Ok(out)
    }

    pub async fn get(&self, id: &str) -> Result<HostedServer> {
        let path = self.server_meta_dir(id)?.join(META_FILE);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|_| Error::Invalid(format!("No server called \"{id}\".")))?;
        parse_json(&bytes, "server.json")
    }

    pub async fn save(&self, server: &HostedServer) -> Result<()> {
        let dir = self.server_meta_dir(&server.id)?;
        tokio::fs::create_dir_all(&dir).await.at(&dir)?;
        let path = dir.join(META_FILE);
        let tmp = dir.join(format!("{META_FILE}.tmp"));
        let json = serde_json::to_vec_pretty(server).expect("server serializes");
        tokio::fs::write(&tmp, json).await.at(&tmp)?;
        tokio::fs::rename(&tmp, &path).await.at(&path)
    }

    /// Makes a new server folder. Nothing is downloaded until it first starts.
    pub async fn create(&self, new: NewServer, instances: &InstanceStore, sources: &Sources) -> Result<Created> {
        if !new.accept_eula {
            return Err(Error::Invalid(
                "Running a Minecraft server needs you to agree to the Minecraft EULA.".into(),
            ));
        }
        if new.game.loader != Loader::Vanilla && new.game.loader_version.is_none() {
            return Err(Error::MissingLoaderVersion(new.game.loader.to_string()));
        }
        let name = match new.name.trim() {
            "" => format!("{} server", new.game.minecraft),
            n => n.to_string(),
        };
        let id = self.unique_id(&name).await;
        let taken: HashSet<u16> = self.list().await?.iter().map(|s| s.port).collect();
        let port = (DEFAULT_PORT..DEFAULT_PORT + 100)
            .find(|p| !taken.contains(p))
            .unwrap_or(DEFAULT_PORT);
        let server = HostedServer {
            id,
            name,
            game_version: new.game.minecraft.clone(),
            loader: new.game.loader,
            loader_version: new.game.loader_version.clone().filter(|_| new.game.loader != Loader::Vanilla),
            memory_mb: DEFAULT_MEMORY_MB,
            port,
            online_mode: new.online_mode,
            whitelist: false,
            tunnel: new.tunnel,
            public_address: None,
            from_instance: new.from_instance.clone(),
            created: now(),
            last_started: None,
        };
        let dir = self.server_dir(&server.id)?;
        tokio::fs::create_dir_all(&dir).await.at(&dir)?;
        tokio::fs::write(dir.join("eula.txt"), "# Agreed in Bagel Time (https://aka.ms/MinecraftEULA)\neula=true\n")
            .await
            .at(&dir)?;
        write_properties(&dir, &server).await?;

        let mut skipped = Vec::new();
        if let Some(instance_id) = &new.from_instance {
            let instance = instances.get(instance_id).await?;
            skipped = copy_from_instance(instances, &instance, &dir, sources).await?;
        }
        self.save(&server).await?;
        Ok(Created { server, skipped })
    }

    /// Writes changed settings into `server.properties` too.
    pub async fn update(&self, server: &HostedServer) -> Result<()> {
        write_properties(&self.server_dir(&server.id)?, server).await?;
        self.save(server).await
    }

    pub async fn mark_started(&self, id: &str) -> Result<HostedServer> {
        let mut server = self.get(id).await?;
        server.last_started = Some(now());
        self.save(&server).await?;
        Ok(server)
    }

    /// Permanently removes the server, including its world.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.get(id).await?;
        let dir = self.server_meta_dir(id)?;
        tokio::fs::remove_dir_all(&dir).await.at(&dir)
    }

    async fn unique_id(&self, name: &str) -> String {
        let base = match slugify(name) {
            s if s.is_empty() => "server".to_string(),
            s => s,
        };
        let mut id = base.clone();
        let mut n = 2;
        while tokio::fs::try_exists(self.dir.join(&id)).await.unwrap_or(false) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

/// Sets the launcher-managed keys in `server.properties`, keeping the rest.
async fn write_properties(dir: &Path, server: &HostedServer) -> Result<()> {
    let path = dir.join("server.properties");
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut values: Vec<(&str, String)> = vec![
        ("server-port", server.port.to_string()),
        ("online-mode", server.online_mode.to_string()),
        ("white-list", server.whitelist.to_string()),
        ("enforce-whitelist", server.whitelist.to_string()),
    ];
    if existing.is_empty() {
        values.push(("motd", server.name.replace(['\n', '\r'], " ")));
    }
    let text = set_properties(&existing, &values);
    tokio::fs::write(&path, text).await.at(&path)
}

/// `server.properties` with `values` set: existing keys are replaced in
/// place, new ones added at the end, comments and other keys kept.
pub fn set_properties(existing: &str, values: &[(&str, String)]) -> String {
    let mut done = HashSet::new();
    let mut out = String::new();
    for line in existing.lines() {
        let key = line.split_once('=').map(|(k, _)| k.trim());
        match key.and_then(|k| values.iter().find(|(name, _)| *name == k)) {
            Some((name, value)) if !line.trim_start().starts_with('#') => {
                out.push_str(&format!("{name}={value}\n"));
                done.insert(*name);
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    for (name, value) in values {
        if !done.contains(name) {
            out.push_str(&format!("{name}={value}\n"));
        }
    }
    out
}

/// Copies an instance's enabled mods and its `config` folder into a server.
/// Mods Modrinth marks as client-only are left out; their titles are returned.
async fn copy_from_instance(
    instances: &InstanceStore,
    instance: &Instance,
    server_dir: &Path,
    sources: &Sources,
) -> Result<Vec<String>> {
    let mut skipped = Vec::new();
    if ContentKind::Mod.supported_by(instance.loader) {
        let mods = InstanceContent::new(instances, instance, ContentKind::Mod)?;
        let list = mods.list().await?;
        let ids: Vec<String> = list
            .iter()
            .filter(|m| m.source == Source::Modrinth)
            .filter_map(|m| m.project_id.clone())
            .collect();
        let client_only: HashMap<String, bool> = sources
            .modrinth
            .projects(&ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.id, p.server_side.as_deref() == Some("unsupported")))
            .collect();
        let dest = server_dir.join("mods");
        tokio::fs::create_dir_all(&dest).await.at(&dest)?;
        for m in list.iter().filter(|m| m.enabled) {
            let only_client = m.project_id.as_ref().and_then(|id| client_only.get(id)).copied().unwrap_or(false);
            if only_client {
                skipped.push(m.title.clone());
                continue;
            }
            let from = mods.dir().join(&m.file_name);
            let to = dest.join(&m.file_name);
            tokio::fs::copy(&from, &to).await.at(&to)?;
        }
    }
    let config = instances.game_dir(instance)?.join("config");
    if config.is_dir() {
        let to = server_dir.join("config");
        tokio::task::spawn_blocking(move || copy_dir(&config, &to)).await??;
    }
    Ok(skipped)
}

/// What a server log line says happened, for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// Finished starting; players can join.
    Ready,
    Joined(String),
    Left(String),
}

/// Reads the few server log lines the launcher cares about.
pub fn server_event(message: &str) -> Option<ServerEvent> {
    let message = crate::logs::message_text(message);
    if message.starts_with("Done (") && message.contains(")! For help") {
        return Some(ServerEvent::Ready);
    }
    let name_ok = |n: &str| !n.is_empty() && n.len() <= 16 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if let Some(name) = message.strip_suffix(" joined the game")
        && name_ok(name)
    {
        return Some(ServerEvent::Joined(name.to_string()));
    }
    if let Some(name) = message.strip_suffix(" left the game")
        && name_ok(name)
    {
        return Some(ServerEvent::Left(name.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_keep_other_keys() {
        let old = "#Minecraft server properties\nmotd=Hi\nserver-port=25565\ndifficulty=hard\n";
        let new = set_properties(old, &[("server-port", "25570".into()), ("online-mode", "false".into())]);
        assert_eq!(
            new,
            "#Minecraft server properties\nmotd=Hi\nserver-port=25570\ndifficulty=hard\nonline-mode=false\n"
        );
    }

    #[test]
    fn reads_server_events() {
        assert_eq!(
            server_event("Done (3.512s)! For help, type \"help\""),
            Some(ServerEvent::Ready)
        );
        assert_eq!(server_event("Kai_99 joined the game"), Some(ServerEvent::Joined("Kai_99".into())));
        assert_eq!(server_event("Kai left the game"), Some(ServerEvent::Left("Kai".into())));
        assert_eq!(
            server_event("[10:54:46] [Server thread/INFO]: Kai joined the game"),
            Some(ServerEvent::Joined("Kai".into()))
        );
        assert_eq!(server_event("<Kai> I joined the game"), None);
    }

    #[tokio::test]
    async fn creates_servers_with_free_ports_and_eula() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path());
        let store = HostStore::new(&paths);
        let instances = InstanceStore::new(&paths);
        let sources = Sources::default();
        let new = |name: &str, eula| NewServer {
            name: name.into(),
            game: GameVersion::vanilla("1.21.4"),
            online_mode: false,
            tunnel: true,
            accept_eula: eula,
            from_instance: None,
        };
        assert!(store.create(new("No", false), &instances, &sources).await.is_err());
        let a = store.create(new("Friends", true), &instances, &sources).await.unwrap().server;
        let b = store.create(new("Friends", true), &instances, &sources).await.unwrap().server;
        assert_eq!((a.id.as_str(), a.port), ("friends", 25565));
        assert_eq!((b.id.as_str(), b.port), ("friends-2", 25566));
        let dir = store.server_dir(&b.id).unwrap();
        assert!(std::fs::read_to_string(dir.join("eula.txt")).unwrap().contains("eula=true"));
        let props = std::fs::read_to_string(dir.join("server.properties")).unwrap();
        assert!(props.contains("server-port=25566") && props.contains("online-mode=false"));
        assert_eq!(store.list().await.unwrap().len(), 2);
    }
}
