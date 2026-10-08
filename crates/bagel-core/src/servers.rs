//! Multiplayer servers: the game's own server list (`servers.dat`), pinging
//! servers for their status, and the servers played on most recently.
//!
//! `servers.dat` is the list the game shows under Multiplayer, so servers
//! added here show up in the game and the other way round. Unknown fields
//! are kept when the file is rewritten.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::Engine;
use fastnbt::Value;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::error::{Error, IoContext, Result};
use crate::paths::Paths;

const DEFAULT_PORT: u16 = 25565;
const TIMEOUT: Duration = Duration::from_secs(5);
/// How many recent servers to remember.
const MAX_RECENT: usize = 30;

/// A server from an instance's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub name: String,
    pub address: String,
    /// PNG as a `data:` URL, saved by the game the last time it pinged.
    pub icon: Option<String>,
}

fn servers_file(game_dir: &Path) -> PathBuf {
    game_dir.join("servers.dat")
}

async fn read_root(game_dir: &Path) -> Result<Value> {
    let path = servers_file(game_dir);
    match tokio::fs::read(&path).await {
        Ok(bytes) => fastnbt::from_bytes(&bytes)
            .map_err(|e| Error::Mods(format!("Couldn't read {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Compound(Default::default())),
        Err(e) => Err(e).at(&path),
    }
}

async fn write_root(game_dir: &Path, root: &Value) -> Result<()> {
    let path = servers_file(game_dir);
    let bytes = fastnbt::to_bytes(root).map_err(|e| Error::Mods(format!("Couldn't save the server list: {e}")))?;
    tokio::fs::create_dir_all(game_dir).await.at(game_dir)?;
    tokio::fs::write(&path, bytes).await.at(&path)
}

/// The `servers` list inside the root compound, created if missing.
fn entries(root: &mut Value) -> Result<&mut Vec<Value>> {
    let Value::Compound(map) = root else {
        return Err(Error::Mods("servers.dat isn't a server list.".into()));
    };
    let list = map.entry("servers".to_string()).or_insert_with(|| Value::List(Vec::new()));
    if !matches!(list, Value::List(_)) {
        *list = Value::List(Vec::new());
    }
    let Value::List(list) = list else { unreachable!() };
    Ok(list)
}

fn string_field(entry: &Value, key: &str) -> Option<String> {
    match entry {
        Value::Compound(map) => match map.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn hidden(entry: &Value) -> bool {
    matches!(entry, Value::Compound(map) if matches!(map.get("hidden"), Some(Value::Byte(1))))
}

/// The servers in the instance's list, in the game's order. Hidden entries
/// (the game's "direct connect" memory) are left out.
pub async fn list(game_dir: &Path) -> Result<Vec<Server>> {
    let mut root = read_root(game_dir).await?;
    Ok(entries(&mut root)?
        .iter()
        .filter(|e| !hidden(e))
        .filter_map(|e| {
            let address = string_field(e, "ip")?;
            Some(Server {
                name: string_field(e, "name").unwrap_or_else(|| address.clone()),
                icon: string_field(e, "icon")
                    .filter(|i| !i.is_empty())
                    .map(|i| format!("data:image/png;base64,{i}")),
                address,
            })
        })
        .collect())
}

/// Adds a server to the end of the list, or renames it if the address is
/// already there.
pub async fn add(game_dir: &Path, name: &str, address: &str) -> Result<()> {
    let address = address.trim();
    parse_address(address)?;
    let name = match name.trim() {
        "" => "Minecraft Server",
        n => n,
    };
    let mut root = read_root(game_dir).await?;
    let list = entries(&mut root)?;
    let existing = list
        .iter_mut()
        .find(|e| !hidden(e) && string_field(e, "ip").is_some_and(|ip| same_address(&ip, address)));
    match existing {
        Some(Value::Compound(map)) => {
            map.insert("name".into(), Value::String(name.into()));
        }
        _ => {
            let mut map = std::collections::HashMap::new();
            map.insert("name".to_string(), Value::String(name.into()));
            map.insert("ip".to_string(), Value::String(address.into()));
            list.push(Value::Compound(map));
        }
    }
    write_root(game_dir, &root).await
}

/// Removes every visible entry with this address.
pub async fn remove(game_dir: &Path, address: &str) -> Result<()> {
    let mut root = read_root(game_dir).await?;
    entries(&mut root)?.retain(|e| hidden(e) || !string_field(e, "ip").is_some_and(|ip| same_address(&ip, address)));
    write_root(game_dir, &root).await
}

/// Host and port as typed: `host`, `host:port`, `[v6]` or `[v6]:port`.
/// The port is `None` when not given, which allows an SRV lookup.
pub fn parse_address(address: &str) -> Result<(String, Option<u16>)> {
    let address = address.trim();
    let bad = || Error::Mods(format!("\"{address}\" isn't a server address."));
    if address.is_empty() || address.contains(char::is_whitespace) {
        return Err(bad());
    }
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or_else(bad)?;
        match after.strip_prefix(':') {
            Some(p) => (host, Some(p)),
            None if after.is_empty() => (host, None),
            None => return Err(bad()),
        }
    } else if address.matches(':').count() > 1 {
        // A bare IPv6 address.
        (address, None)
    } else {
        match address.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (address, None),
        }
    };
    if host.is_empty() {
        return Err(bad());
    }
    let port = port.map(|p| p.parse::<u16>().map_err(|_| bad())).transpose()?;
    Ok((host.to_lowercase(), port))
}

/// Whether two typed addresses point at the same place ("a.net" and
/// "A.net:25565" do).
pub fn same_address(a: &str, b: &str) -> bool {
    match (parse_address(a), parse_address(b)) {
        (Ok((ha, pa)), Ok((hb, pb))) => ha == hb && pa.unwrap_or(DEFAULT_PORT) == pb.unwrap_or(DEFAULT_PORT),
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

/// Where to actually connect: follows the `_minecraft._tcp` SRV record when
/// no port was typed, like the game does.
pub async fn resolve(address: &str) -> Result<(String, u16)> {
    let (host, port) = parse_address(address)?;
    if let Some(port) = port {
        return Ok((host, port));
    }
    if host.parse::<std::net::IpAddr>().is_err()
        && let Ok(builder) = hickory_resolver::TokioResolver::builder_tokio()
        && let Ok(resolver) = builder.build()
        && let Ok(Ok(lookup)) =
            tokio::time::timeout(TIMEOUT, resolver.srv_lookup(format!("_minecraft._tcp.{host}."))).await
    {
        for record in lookup.answers() {
            if let hickory_resolver::proto::rr::RData::SRV(srv) = &record.data {
                let target = srv.target.to_utf8();
                return Ok((target.trim_end_matches('.').to_string(), srv.port));
            }
        }
    }
    Ok((host, DEFAULT_PORT))
}

/// What a server says about itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// Message of the day, with `§` formatting codes.
    pub motd: String,
    pub online: u32,
    pub max: u32,
    /// Some player names, when the server shares them.
    pub sample: Vec<String>,
    /// e.g. "Paper 1.21.4".
    pub version: String,
    pub protocol: i32,
    /// PNG as a `data:` URL.
    pub icon: Option<String>,
    pub ping_ms: u32,
}

/// Asks a server for its status (the same "server list ping" the game uses).
pub async fn ping(address: &str) -> Result<ServerStatus> {
    let (host, port) = resolve(address).await?;
    let offline = |e: &dyn std::fmt::Display| Error::Mods(format!("Can't reach the server: {e}"));
    let result = tokio::time::timeout(TIMEOUT * 2, async {
        let mut stream = tokio::time::timeout(TIMEOUT, TcpStream::connect((host.as_str(), port)))
            .await
            .map_err(|_| offline(&"timed out"))?
            .map_err(|e| offline(&e))?;
        let _ = stream.set_nodelay(true);

        // Handshake (next state 1 = status), then a status request.
        let mut handshake = Vec::new();
        write_varint(&mut handshake, 0);
        // Some servers drop pings that send -1, so claim a recent version.
        write_varint(&mut handshake, 769);
        write_string(&mut handshake, &host);
        handshake.extend_from_slice(&port.to_be_bytes());
        write_varint(&mut handshake, 1);
        let mut out = frame(&handshake);
        out.extend(frame(&[0]));
        stream.write_all(&out).await.map_err(|e| offline(&e))?;

        let started = Instant::now();
        let packet = read_packet(&mut stream).await.map_err(|e| offline(&e))?;
        let mut first_reply = started.elapsed();
        let mut cursor = packet.as_slice();
        if read_varint_slice(&mut cursor)? != 0 {
            return Err(Error::Mods("The server sent an unexpected reply.".into()));
        }
        let len = read_varint_slice(&mut cursor)? as usize;
        let json = cursor.get(..len).ok_or_else(|| Error::Mods("The server's reply was cut off.".into()))?;
        let status: RawStatus = serde_json::from_slice(json)
            .map_err(|e| Error::Mods(format!("The server's reply didn't make sense: {e}")))?;

        // Ping/pong for a fair latency; fall back to the status time.
        let mut ping = vec![1];
        ping.extend_from_slice(&0i64.to_be_bytes());
        let sent = Instant::now();
        if stream.write_all(&frame(&ping)).await.is_ok()
            && tokio::time::timeout(Duration::from_secs(2), read_packet(&mut stream))
                .await
                .is_ok_and(|r| r.is_ok())
        {
            first_reply = sent.elapsed();
        }
        Ok(status.into_status(first_reply))
    })
    .await;
    result.map_err(|_| offline(&"timed out"))?
}

#[derive(Deserialize)]
struct RawStatus {
    #[serde(default)]
    description: serde_json::Value,
    #[serde(default)]
    players: Option<RawPlayers>,
    #[serde(default)]
    version: Option<RawVersion>,
    #[serde(default)]
    favicon: Option<String>,
}

#[derive(Deserialize)]
struct RawPlayers {
    #[serde(default)]
    max: u32,
    #[serde(default)]
    online: u32,
    #[serde(default)]
    sample: Vec<RawPlayer>,
}

#[derive(Deserialize)]
struct RawPlayer {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct RawVersion {
    #[serde(default)]
    name: String,
    #[serde(default)]
    protocol: i32,
}

impl RawStatus {
    fn into_status(self, ping: Duration) -> ServerStatus {
        let players = self.players.unwrap_or(RawPlayers { max: 0, online: 0, sample: Vec::new() });
        let version = self.version.unwrap_or(RawVersion { name: String::new(), protocol: 0 });
        ServerStatus {
            motd: chat_to_legacy(&self.description),
            online: players.online,
            max: players.max,
            sample: players.sample.into_iter().map(|p| p.name).filter(|n| !n.is_empty()).collect(),
            version: version.name,
            protocol: version.protocol,
            icon: self.favicon.filter(|f| f.starts_with("data:image/") && base64_ok(f)),
            ping_ms: ping.as_millis().min(u32::MAX as u128) as u32,
        }
    }
}

fn base64_ok(data_url: &str) -> bool {
    data_url
        .split_once(',')
        .is_some_and(|(_, b64)| base64::engine::general_purpose::STANDARD.decode(b64.replace('\n', "")).is_ok())
}

/// A chat component (or plain string) as text with `§` codes, so the UI
/// renders every MOTD the same way.
fn chat_to_legacy(value: &serde_json::Value) -> String {
    const COLORS: [(&str, char); 16] = [
        ("black", '0'),
        ("dark_blue", '1'),
        ("dark_green", '2'),
        ("dark_aqua", '3'),
        ("dark_red", '4'),
        ("dark_purple", '5'),
        ("gold", '6'),
        ("gray", '7'),
        ("dark_gray", '8'),
        ("blue", '9'),
        ("green", 'a'),
        ("aqua", 'b'),
        ("red", 'c'),
        ("light_purple", 'd'),
        ("yellow", 'e'),
        ("white", 'f'),
    ];
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            serde_json::Value::Object(map) => {
                if let Some(color) = map.get("color").and_then(|c| c.as_str()) {
                    if let Some((_, code)) = COLORS.iter().find(|(n, _)| *n == color) {
                        out.push('§');
                        out.push(*code);
                    } else if color.starts_with('#') && color.len() == 7 {
                        // Hex colours: kept as a custom marker the UI understands.
                        out.push_str("§#");
                        out.push_str(&color[1..]);
                    }
                }
                for (key, code) in [("bold", 'l'), ("italic", 'o'), ("underlined", 'n'), ("strikethrough", 'm')] {
                    if map.get(key).and_then(|b| b.as_bool()) == Some(true) {
                        out.push('§');
                        out.push(code);
                    }
                }
                if let Some(text) = map.get("text").and_then(|t| t.as_str()) {
                    out.push_str(text);
                }
                if let Some(extra) = map.get("extra") {
                    walk(extra, out);
                }
                out.push_str("§r");
            }
            _ => {}
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    out.trim_end_matches("§r").to_string()
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    loop {
        if v & !0x7F == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8 & 0x7F) | 0x80);
        v >>= 7;
    }
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    write_varint(out, s.len() as i32);
    out.extend_from_slice(s.as_bytes());
}

/// A packet with its length in front.
fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 5);
    write_varint(&mut out, body.len() as i32);
    out.extend_from_slice(body);
    out
}

fn read_varint_slice(data: &mut &[u8]) -> Result<i32> {
    let mut value = 0u32;
    for i in 0..5 {
        let (&byte, rest) = data
            .split_first()
            .ok_or_else(|| Error::Mods("The server's reply was cut off.".into()))?;
        *data = rest;
        value |= ((byte & 0x7F) as u32) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(Error::Mods("The server sent a malformed number.".into()))
}

async fn read_packet(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = 0u32;
    for i in 0..5 {
        let byte = stream.read_u8().await?;
        len |= ((byte & 0x7F) as u32) << (7 * i);
        if byte & 0x80 == 0 {
            break;
        }
    }
    // A status reply with a big icon is well under this.
    if len > 4 * 1024 * 1024 {
        return Err(std::io::Error::other("reply too large"));
    }
    let mut buf = vec![0; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

/// A server played on recently, from any instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentServer {
    pub instance_id: String,
    pub name: String,
    pub address: String,
    /// Unix seconds, like `Instance::last_played`.
    pub last_played: u64,
}

/// Joins can be recorded from the launch and the game log at once.
static RECENT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn recent_file(paths: &Paths) -> PathBuf {
    paths.root().join("recent-servers.json")
}

/// Newest first.
pub async fn recent(paths: &Paths) -> Vec<RecentServer> {
    match tokio::fs::read(recent_file(paths)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Remembers that an instance just joined a server. `address` is matched
/// against the instance's list so the entry gets its name (and the address
/// as typed there, rather than where an SRV record led).
pub async fn record_join(paths: &Paths, instance_id: &str, game_dir: &Path, address: &str) -> Result<()> {
    let _guard = RECENT_LOCK.lock().await;
    let mut list = recent(paths).await;
    // The instance's list first, then servers it joined before (both may
    // name the server differently from where the game says it connected).
    let mut known = self::list(game_dir).await.unwrap_or_default();
    known.extend(list.iter().filter(|r| r.instance_id == instance_id).map(|r| Server {
        name: r.name.clone(),
        address: r.address.clone(),
        icon: None,
    }));
    let mut matched = known.iter().find(|s| same_address(&s.address, address)).cloned();
    if matched.is_none() {
        // The game logs where it connected after following SRV records.
        for s in &known {
            if let Ok((host, port)) = resolve(&s.address).await
                && same_address(&format!("{host}:{port}"), address)
            {
                matched = Some(s.clone());
                break;
            }
        }
    }
    let (name, address) = match matched {
        Some(s) => (s.name, s.address),
        None => (address.to_string(), address.to_string()),
    };

    list.retain(|r| !(r.instance_id == instance_id && same_address(&r.address, &address)));
    list.insert(
        0,
        RecentServer {
            instance_id: instance_id.to_string(),
            name,
            address,
            last_played: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        },
    );
    list.truncate(MAX_RECENT);
    let path = recent_file(paths);
    let json = serde_json::to_vec_pretty(&list).map_err(|e| Error::Mods(e.to_string()))?;
    tokio::fs::write(&path, json).await.at(&path)
}

/// Forgets recent entries for instances that no longer exist.
pub async fn forget_instance(paths: &Paths, instance_id: &str) -> Result<()> {
    let _guard = RECENT_LOCK.lock().await;
    let mut list = recent(paths).await;
    let before = list.len();
    list.retain(|r| r.instance_id != instance_id);
    if list.len() == before {
        return Ok(());
    }
    let path = recent_file(paths);
    let json = serde_json::to_vec_pretty(&list).map_err(|e| Error::Mods(e.to_string()))?;
    tokio::fs::write(&path, json).await.at(&path)
}

/// The server address in a game log line like "Connecting to mc.example.net, 25565".
pub fn joined_from_log(message: &str) -> Option<String> {
    let rest = message.strip_prefix("Connecting to ")?;
    let (host, port) = rest.rsplit_once(", ")?;
    let port: u16 = port.trim().parse().ok()?;
    let host = host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    Some(if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_addresses() {
        assert_eq!(parse_address("Play.Example.net").unwrap(), ("play.example.net".into(), None));
        assert_eq!(parse_address("a.net:25570").unwrap(), ("a.net".into(), Some(25570)));
        assert_eq!(parse_address("[::1]:25566").unwrap(), ("::1".into(), Some(25566)));
        assert_eq!(parse_address("::1").unwrap(), ("::1".into(), None));
        assert!(parse_address("a.net:notaport").is_err());
        assert!(parse_address("").is_err());
        assert!(parse_address("has space").is_err());
        assert!(same_address("A.net", "a.net:25565"));
        assert!(!same_address("a.net", "a.net:25566"));
    }

    #[test]
    fn reads_join_lines() {
        assert_eq!(joined_from_log("Connecting to mc.hypixel.net, 25565").as_deref(), Some("mc.hypixel.net:25565"));
        assert_eq!(joined_from_log("Connecting to ::1, 25565").as_deref(), Some("[::1]:25565"));
        assert_eq!(joined_from_log("Connecting to the world"), None);
    }

    #[test]
    fn motd_components_become_codes() {
        let json = serde_json::json!({"text": "", "extra": [{"text": "Hi", "color": "gold", "bold": true}, " there"]});
        assert_eq!(chat_to_legacy(&json), "§6§lHi§r there");
        assert_eq!(chat_to_legacy(&serde_json::json!("§aPlain")), "§aPlain");
    }

    #[test]
    fn varints_round_trip() {
        for v in [0, 1, 127, 128, 255, 25565, -1, i32::MAX] {
            let mut buf = Vec::new();
            write_varint(&mut buf, v);
            assert_eq!(read_varint_slice(&mut buf.as_slice()).unwrap(), v);
        }
    }

    #[tokio::test]
    async fn server_list_round_trip_keeps_other_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // A file the game wrote: one hidden direct-connect entry and one with extra fields.
        let mut first = std::collections::HashMap::new();
        first.insert("name".to_string(), Value::String("Friends".into()));
        first.insert("ip".to_string(), Value::String("friends.example.net".into()));
        first.insert("acceptTextures".to_string(), Value::Byte(1));
        let mut direct = std::collections::HashMap::new();
        direct.insert("name".to_string(), Value::String("Minecraft Server".into()));
        direct.insert("ip".to_string(), Value::String("10.0.0.5".into()));
        direct.insert("hidden".to_string(), Value::Byte(1));
        let mut root = std::collections::HashMap::new();
        root.insert("servers".to_string(), Value::List(vec![Value::Compound(first), Value::Compound(direct)]));
        write_root(dir, &Value::Compound(root)).await.unwrap();

        assert_eq!(list(dir).await.unwrap().len(), 1);
        add(dir, "Hypixel", "mc.hypixel.net").await.unwrap();
        add(dir, "Friends SMP", "FRIENDS.example.net:25565").await.unwrap();
        let servers = list(dir).await.unwrap();
        assert_eq!(
            servers.iter().map(|s| (s.name.as_str(), s.address.as_str())).collect::<Vec<_>>(),
            [("Friends SMP", "friends.example.net"), ("Hypixel", "mc.hypixel.net")]
        );
        remove(dir, "mc.hypixel.net:25565").await.unwrap();
        assert_eq!(list(dir).await.unwrap().len(), 1);

        let mut root = read_root(dir).await.unwrap();
        let entries = entries(&mut root).unwrap();
        assert_eq!(entries.len(), 2, "hidden entry kept");
        assert!(matches!(&entries[0], Value::Compound(m) if m.get("acceptTextures") == Some(&Value::Byte(1))));
    }
}
