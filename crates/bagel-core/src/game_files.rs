//! Things the game writes into an instance folder: worlds, screenshots and
//! logs.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

use crate::error::{Error, IoContext, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Folder name inside `saves`.
    pub folder: String,
    /// Name shown in the game, from `level.dat`.
    pub name: String,
    /// Unix milliseconds.
    pub last_played: Option<u64>,
    /// survival, creative, adventure or spectator.
    pub game_mode: Option<String>,
    pub hardcore: bool,
    /// Minecraft version that last saved it, e.g. "1.21.4".
    pub version: Option<String>,
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub file_name: String,
    pub path: PathBuf,
    /// Unix milliseconds.
    pub modified: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    /// Path relative to the game folder, e.g. `logs/latest.log`.
    pub name: String,
    pub crash_report: bool,
    /// Unix milliseconds.
    pub modified: u64,
    pub size: u64,
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

/// Files (not folders) in `dir`, with their metadata. Missing folder = empty.
fn files_in(dir: &Path) -> Result<Vec<(String, std::fs::Metadata)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).at(dir),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let (Some(name), Ok(meta)) = (entry.file_name().to_str().map(String::from), entry.metadata()) else {
            continue;
        };
        out.push((name, meta));
    }
    Ok(out)
}

/// Worlds in `saves`, most recently played first.
pub async fn worlds(game_dir: &Path) -> Result<Vec<World>> {
    let saves = game_dir.join("saves");
    tokio::task::spawn_blocking(move || {
        let mut out: Vec<World> = files_in(&saves)?
            .into_iter()
            .filter(|(_, meta)| meta.is_dir())
            .filter_map(|(folder, _)| read_world(&saves.join(&folder), folder))
            .collect();
        out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
        Ok(out)
    })
    .await?
}

fn read_world(dir: &Path, folder: String) -> Option<World> {
    let level = dir.join("level.dat");
    let meta = std::fs::metadata(&level).ok()?;
    let info = std::fs::read(&level).ok().and_then(|b| level_info(&b)).unwrap_or_default();
    let icon = dir.join("icon.png");
    Some(World {
        name: info.name.unwrap_or_else(|| folder.clone()),
        folder,
        last_played: info.last_played.or(Some(modified_ms(&meta))),
        game_mode: info.game_type.map(|t| {
            match t {
                1 => "creative",
                2 => "adventure",
                3 => "spectator",
                _ => "survival",
            }
            .to_string()
        }),
        hardcore: info.hardcore,
        version: info.version,
        icon: icon.is_file().then_some(icon),
    })
}

/// Screenshots, newest first.
pub async fn screenshots(game_dir: &Path) -> Result<Vec<Screenshot>> {
    let dir = game_dir.join("screenshots");
    tokio::task::spawn_blocking(move || {
        let mut out: Vec<Screenshot> = files_in(&dir)?
            .into_iter()
            .filter(|(name, meta)| meta.is_file() && is_image(name))
            .map(|(name, meta)| Screenshot {
                path: dir.join(&name),
                file_name: name,
                modified: modified_ms(&meta),
                size: meta.len(),
            })
            .collect();
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(out)
    })
    .await?
}

fn is_image(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e))
}

pub async fn delete_screenshot(game_dir: &Path, file_name: &str) -> Result<()> {
    if !is_plain_name(file_name) || !is_image(file_name) {
        return Err(Error::Invalid(format!("\"{file_name}\" isn't a screenshot.")));
    }
    let path = game_dir.join("screenshots").join(file_name);
    tokio::fs::remove_file(&path).await.at(&path)
}

/// Log files and crash reports, newest first.
pub async fn log_files(game_dir: &Path) -> Result<Vec<LogFile>> {
    let game_dir = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for (folder, crash) in [("logs", false), ("crash-reports", true)] {
            for (name, meta) in files_in(&game_dir.join(folder))? {
                let lower = name.to_ascii_lowercase();
                let wanted = if crash {
                    lower.ends_with(".txt")
                } else {
                    lower.ends_with(".log") || lower.ends_with(".log.gz")
                };
                if meta.is_file() && wanted {
                    out.push(LogFile {
                        name: format!("{folder}/{name}"),
                        crash_report: crash,
                        modified: modified_ms(&meta),
                        size: meta.len(),
                    });
                }
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(out)
    })
    .await?
}

/// At most this much of a log is returned (the end of it).
const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;

/// Reads a log or crash report (unzipping `.gz`). `name` comes from
/// [`log_files`].
pub async fn read_log(game_dir: &Path, name: &str) -> Result<String> {
    let (folder, file) = name
        .split_once('/')
        .filter(|(folder, file)| matches!(*folder, "logs" | "crash-reports") && is_plain_name(file))
        .ok_or_else(|| Error::Invalid(format!("\"{name}\" isn't a log file.")))?;
    let path = game_dir.join(folder).join(file);
    tokio::task::spawn_blocking(move || {
        let raw = std::fs::read(&path).at(&path)?;
        let mut bytes = if path.extension().is_some_and(|e| e == "gz") {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out).at(&path)?;
            out
        } else {
            raw
        };
        if bytes.len() > MAX_LOG_BYTES {
            bytes.drain(..bytes.len() - MAX_LOG_BYTES);
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await?
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\', ':'])
}

#[derive(Debug, Default, PartialEq)]
struct LevelInfo {
    name: Option<String>,
    last_played: Option<u64>,
    game_type: Option<i32>,
    hardcore: bool,
    version: Option<String>,
}

/// Pulls a few fields out of a gzipped NBT `level.dat`.
fn level_info(gz: &[u8]) -> Option<LevelInfo> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut raw).ok()?;
    let mut r = Nbt { data: &raw, pos: 0 };
    // Root: a named compound containing a "Data" compound.
    if r.u8()? != TAG_COMPOUND {
        return None;
    }
    r.string()?;
    let mut info = LevelInfo::default();
    loop {
        let tag = r.u8()?;
        if tag == TAG_END {
            return Some(info);
        }
        let name = r.string()?;
        if tag == TAG_COMPOUND && name == "Data" {
            r.read_data(&mut info)?;
            return Some(info);
        }
        r.skip(tag)?;
    }
}

const TAG_END: u8 = 0;
const TAG_COMPOUND: u8 = 10;

struct Nbt<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Nbt<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let len = self.i16()? as u16 as usize;
        // Java's "modified UTF-8" is plain UTF-8 for everything but rare cases.
        Some(String::from_utf8_lossy(self.take(len)?).into_owned())
    }
    fn len(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }

    /// Reads the fields we want from the "Data" compound, skipping the rest.
    fn read_data(&mut self, info: &mut LevelInfo) -> Option<()> {
        loop {
            let tag = self.u8()?;
            if tag == TAG_END {
                return Some(());
            }
            let name = self.string()?;
            match (tag, name.as_str()) {
                (8, "LevelName") => info.name = Some(self.string()?),
                (4, "LastPlayed") => info.last_played = u64::try_from(self.i64()?).ok(),
                (3, "GameType") => info.game_type = Some(self.i32()?),
                (1, "hardcore") => info.hardcore = self.u8()? != 0,
                (TAG_COMPOUND, "Version") => {
                    // { Name: "1.21.4", Id: ..., Snapshot: ... }
                    loop {
                        let t = self.u8()?;
                        if t == TAG_END {
                            break;
                        }
                        let n = self.string()?;
                        if t == 8 && n == "Name" {
                            info.version = Some(self.string()?);
                        } else {
                            self.skip(t)?;
                        }
                    }
                }
                _ => self.skip(tag)?,
            }
        }
    }

    fn skip(&mut self, tag: u8) -> Option<()> {
        match tag {
            1 => drop(self.take(1)?),
            2 => drop(self.take(2)?),
            3 | 5 => drop(self.take(4)?),
            4 | 6 => drop(self.take(8)?),
            7 => {
                let n = self.len()?;
                self.take(n)?;
            }
            8 => drop(self.string()?),
            9 => {
                let inner = self.u8()?;
                let n = self.len()?;
                for _ in 0..n {
                    self.skip(inner)?;
                }
            }
            TAG_COMPOUND => loop {
                let t = self.u8()?;
                if t == TAG_END {
                    break;
                }
                self.string()?;
                self.skip(t)?;
            },
            11 => {
                let n = self.len()?;
                self.take(n.checked_mul(4)?)?;
            }
            12 => {
                let n = self.len()?;
                self.take(n.checked_mul(8)?)?;
            }
            _ => return None,
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Builds a small gzipped level.dat by hand.
    fn level_dat() -> Vec<u8> {
        fn name(out: &mut Vec<u8>, tag: u8, n: &str) {
            out.push(tag);
            out.extend((n.len() as u16).to_be_bytes());
            out.extend(n.as_bytes());
        }
        fn string(out: &mut Vec<u8>, s: &str) {
            out.extend((s.len() as u16).to_be_bytes());
            out.extend(s.as_bytes());
        }
        let mut nbt = Vec::new();
        name(&mut nbt, 10, "");
        name(&mut nbt, 10, "Data");
        name(&mut nbt, 9, "ServerBrands"); // list of strings, skipped
        nbt.push(8);
        nbt.extend(1i32.to_be_bytes());
        string(&mut nbt, "fabric");
        name(&mut nbt, 8, "LevelName");
        string(&mut nbt, "My Base");
        name(&mut nbt, 11, "Pos"); // int array, skipped
        nbt.extend(2i32.to_be_bytes());
        nbt.extend([0; 8]);
        name(&mut nbt, 4, "LastPlayed");
        nbt.extend(1_700_000_000_000i64.to_be_bytes());
        name(&mut nbt, 3, "GameType");
        nbt.extend(1i32.to_be_bytes());
        name(&mut nbt, 1, "hardcore");
        nbt.push(0);
        name(&mut nbt, 10, "Version");
        name(&mut nbt, 3, "Id");
        nbt.extend(4189i32.to_be_bytes());
        name(&mut nbt, 8, "Name");
        string(&mut nbt, "1.21.4");
        nbt.push(0); // end Version
        nbt.push(0); // end Data
        nbt.push(0); // end root

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn reads_level_dat() {
        let info = level_info(&level_dat()).unwrap();
        assert_eq!(
            info,
            LevelInfo {
                name: Some("My Base".into()),
                last_played: Some(1_700_000_000_000),
                game_type: Some(1),
                hardcore: false,
                version: Some("1.21.4".into()),
            }
        );
        assert!(level_info(b"not gzip").is_none());
    }

    #[tokio::test]
    async fn lists_worlds_screenshots_and_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path();
        let world = game.join("saves").join("base");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), level_dat()).unwrap();
        std::fs::create_dir_all(game.join("saves").join("not-a-world")).unwrap();
        let worlds = worlds(game).await.unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name, "My Base");
        assert_eq!(worlds[0].game_mode.as_deref(), Some("creative"));

        std::fs::create_dir_all(game.join("screenshots")).unwrap();
        std::fs::write(game.join("screenshots").join("2026-01-01.png"), b"x").unwrap();
        std::fs::write(game.join("screenshots").join("notes.txt"), b"x").unwrap();
        assert_eq!(screenshots(game).await.unwrap().len(), 1);
        assert!(delete_screenshot(game, "../level.dat").await.is_err());
        delete_screenshot(game, "2026-01-01.png").await.unwrap();

        std::fs::create_dir_all(game.join("logs")).unwrap();
        std::fs::write(game.join("logs").join("latest.log"), b"hello").unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"old log").unwrap();
        std::fs::write(game.join("logs").join("2026-01-01-1.log.gz"), gz.finish().unwrap()).unwrap();
        let logs = log_files(game).await.unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(read_log(game, "logs/2026-01-01-1.log.gz").await.unwrap(), "old log");
        assert_eq!(read_log(game, "logs/latest.log").await.unwrap(), "hello");
        assert!(read_log(game, "saves/base/level.dat").await.is_err());
        assert!(read_log(game, "logs/../options.txt").await.is_err());
    }
}
