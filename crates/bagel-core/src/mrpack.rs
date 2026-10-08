//! Modrinth modpacks (`.mrpack`): a zip with `modrinth.index.json` listing
//! files to download, plus `overrides/` and `client-overrides/` folders copied
//! into the game folder.
//!
//! Format: https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::DownloadJob;
use crate::error::{Error, IoContext, Result, parse_json};
use crate::instance::{Instance, InstanceStore};
use crate::loaders::{GameVersion, Loader};
use crate::modrinth::{Modrinth, pick_best};
use crate::mods::InstanceMods;
use crate::paths::Paths;
use crate::progress::Progress;

const INDEX: &str = "modrinth.index.json";

/// Hosts the format allows packs to download from.
const ALLOWED_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    pub files: Vec<PackFile>,
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub path: String,
    pub hashes: HashMap<String, String>,
    #[serde(default)]
    pub env: Option<PackEnv>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackEnv {
    /// required, optional or unsupported.
    pub client: String,
}

/// Reads the index from a `.mrpack` file.
pub async fn read_index(pack: &Path) -> Result<PackIndex> {
    let pack = pack.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&pack).at(&pack)?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|_| Error::Pack("That file isn't a Modrinth modpack (.mrpack).".into()))?;
        let mut entry = zip
            .by_name(INDEX)
            .map_err(|_| Error::Pack("That file isn't a Modrinth modpack: modrinth.index.json is missing.".into()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).at(&pack)?;
        parse_json(&bytes, INDEX)
    })
    .await?
}

/// The Minecraft version and loader a pack needs.
pub fn pack_game(index: &PackIndex) -> Result<GameVersion> {
    if index.game != "minecraft" {
        return Err(Error::Pack(format!("This pack is for \"{}\", not Minecraft.", index.game)));
    }
    if index.format_version != 1 {
        return Err(Error::Pack(format!(
            "This pack uses format version {}, which Bagel Time doesn't understand yet.",
            index.format_version
        )));
    }
    let deps = &index.dependencies;
    let minecraft = deps
        .get("minecraft")
        .ok_or_else(|| Error::Pack("The pack doesn't say which Minecraft version it needs.".into()))?;
    for (key, name) in [("forge", "Forge"), ("neoforge", "NeoForge")] {
        if deps.contains_key(key) {
            return Err(Error::Pack(format!(
                "This pack uses {name}, which Bagel Time doesn't support yet. It's coming in a later update."
            )));
        }
    }
    let (loader, loader_version) = if let Some(v) = deps.get("fabric-loader") {
        (Loader::Fabric, Some(v.clone()))
    } else if let Some(v) = deps.get("quilt-loader") {
        (Loader::Quilt, Some(v.clone()))
    } else {
        (Loader::Vanilla, None)
    };
    Ok(GameVersion {
        minecraft: minecraft.clone(),
        loader,
        loader_version,
    })
}

/// A relative path that stays inside the folder it's joined to.
pub(crate) fn safe_relative_path(p: &str) -> Option<PathBuf> {
    if p.starts_with(['/', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in p.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return None;
        }
        out.push(part);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn allowed_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
    })
}

/// Files the client needs, as downloads into `game_dir`.
fn download_jobs(index: &PackIndex, game_dir: &Path) -> Result<Vec<DownloadJob>> {
    let mut jobs = Vec::new();
    for file in &index.files {
        if file.env.as_ref().is_some_and(|e| e.client == "unsupported") {
            continue;
        }
        let rel = safe_relative_path(&file.path)
            .ok_or_else(|| Error::Pack(format!("The pack tries to write outside its folder: {}", file.path)))?;
        let url = file
            .downloads
            .iter()
            .find(|u| allowed_url(u))
            .ok_or_else(|| Error::Pack(format!("{} has no download from a trusted site.", file.path)))?;
        let sha1 = file
            .hashes
            .get("sha1")
            .ok_or_else(|| Error::Pack(format!("{} has no SHA-1 hash.", file.path)))?;
        jobs.push(DownloadJob {
            url: url.clone(),
            path: game_dir.join(rel),
            sha1: Some(sha1.clone()),
            size: Some(file.file_size),
        });
    }
    Ok(jobs)
}

/// Copies `overrides/` and then `client-overrides/` into the game folder.
fn extract_overrides(pack: &Path, game_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(pack).at(pack)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let Some(rel) = entry.name().strip_prefix(prefix) else {
                continue;
            };
            let rel = safe_relative_path(rel)
                .ok_or_else(|| Error::Pack(format!("The pack tries to write outside its folder: {}", entry.name())))?;
            let dest = game_dir.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            let mut out = std::fs::File::create(&dest).at(&dest)?;
            std::io::copy(&mut entry, &mut out).at(&dest)?;
        }
    }
    Ok(())
}

/// Creates a new instance from a `.mrpack` file. The instance is removed
/// again if anything fails part-way.
pub async fn install_pack(
    store: &InstanceStore,
    modrinth: &Modrinth,
    pack: &Path,
    progress: &Progress,
) -> Result<Instance> {
    progress.stage("Reading modpack", 0);
    let index = read_index(pack).await?;
    let game = pack_game(&index)?;
    // Check every path before creating anything.
    download_jobs(&index, Path::new("."))?;

    let instance = store.create(&index.name, game).await?;
    match fill_instance(store, modrinth, &instance, &index, pack, progress).await {
        Ok(()) => Ok(instance),
        Err(e) => {
            let _ = store.delete(&instance.id).await;
            Err(e)
        }
    }
}

async fn fill_instance(
    store: &InstanceStore,
    modrinth: &Modrinth,
    instance: &Instance,
    index: &PackIndex,
    pack: &Path,
    progress: &Progress,
) -> Result<()> {
    let game_dir = store.game_dir(instance)?;
    let jobs = download_jobs(index, &game_dir)?;
    modrinth.downloader().fetch_all("Downloading mods", jobs, progress).await?;

    progress.stage("Unpacking files", 0);
    let (pack, dir) = (pack.to_path_buf(), game_dir.clone());
    tokio::task::spawn_blocking(move || extract_overrides(&pack, &dir)).await??;

    // Names and icons are nice to have; the pack works without them.
    progress.stage("Looking up mods", 0);
    let _ = InstanceMods::new(store, instance)?.identify(modrinth).await;
    Ok(())
}

/// Downloads a modpack from Modrinth (the given version, or the newest
/// release) and installs it as a new instance.
pub async fn install_from_modrinth(
    paths: &Paths,
    store: &InstanceStore,
    modrinth: &Modrinth,
    project_id: &str,
    version_id: Option<&str>,
    progress: &Progress,
) -> Result<Instance> {
    progress.stage("Finding the latest version", 0);
    let version = match version_id {
        Some(id) => modrinth.version(id).await?,
        None => {
            let versions = modrinth.project_versions(project_id, &[], &[]).await?;
            pick_best(&versions)
                .cloned()
                .ok_or_else(|| Error::Pack("This modpack has no versions to download.".into()))?
        }
    };
    let file = version
        .files
        .iter()
        .find(|f| f.filename.ends_with(".mrpack"))
        .ok_or_else(|| Error::Pack("This modpack version has no .mrpack file.".into()))?;

    let tmp = paths.cache_dir().join(format!("{}.mrpack", file.hashes.sha1));
    progress.stage("Downloading modpack", 0);
    modrinth
        .downloader()
        .fetch(&DownloadJob {
            url: file.url.clone(),
            path: tmp.clone(),
            sha1: Some(file.hashes.sha1.clone()),
            size: Some(file.size),
        })
        .await?;
    let result = install_pack(store, modrinth, &tmp, progress).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn index(deps: &[(&str, &str)], files: serde_json::Value) -> PackIndex {
        let deps: HashMap<String, String> = deps.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        serde_json::from_value(serde_json::json!({
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test Pack",
            "files": files,
            "dependencies": deps
        }))
        .unwrap()
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_relative_path("mods/a.jar"), Some(PathBuf::from("mods").join("a.jar")));
        assert_eq!(safe_relative_path("config\\x.toml"), Some(PathBuf::from("config").join("x.toml")));
        for bad in ["../evil.jar", "mods/../../x", "/etc/passwd", "\\x", "C:/Windows/x", "C:x", "", "./"] {
            assert_eq!(safe_relative_path(bad), None, "{bad}");
        }
    }

    #[test]
    fn game_from_dependencies() {
        let fabric = pack_game(&index(&[("minecraft", "1.21.4"), ("fabric-loader", "0.16.10")], serde_json::json!([])))
            .unwrap();
        assert_eq!(fabric.loader, Loader::Fabric);
        assert_eq!(fabric.loader_version.as_deref(), Some("0.16.10"));
        let vanilla = pack_game(&index(&[("minecraft", "1.21.4")], serde_json::json!([]))).unwrap();
        assert_eq!(vanilla.loader, Loader::Vanilla);
        let forge = pack_game(&index(&[("minecraft", "1.20.1"), ("forge", "47.3.0")], serde_json::json!([])));
        assert!(forge.unwrap_err().to_string().contains("Forge"));
        assert!(pack_game(&index(&[], serde_json::json!([]))).is_err());
    }

    #[test]
    fn download_jobs_check_paths_hosts_and_env() {
        let file = |path: &str, url: &str, client: &str| {
            serde_json::json!({
                "path": path,
                "hashes": {"sha1": "aa", "sha512": "bb"},
                "env": {"client": client, "server": "required"},
                "downloads": [url],
                "fileSize": 3
            })
        };
        let ok = index(
            &[("minecraft", "1.21.4")],
            serde_json::json!([
                file("mods/a.jar", "https://cdn.modrinth.com/a.jar", "required"),
                file("mods/server-only.jar", "https://cdn.modrinth.com/s.jar", "unsupported"),
            ]),
        );
        let jobs = download_jobs(&ok, Path::new("game")).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].path, Path::new("game").join("mods").join("a.jar"));

        let escape = index(
            &[("minecraft", "1.21.4")],
            serde_json::json!([file("../a.jar", "https://cdn.modrinth.com/a.jar", "required")]),
        );
        assert!(download_jobs(&escape, Path::new("game")).is_err());

        let host = index(
            &[("minecraft", "1.21.4")],
            serde_json::json!([file("mods/a.jar", "https://evil.example.com/a.jar", "required")]),
        );
        assert!(download_jobs(&host, Path::new("game")).is_err());
        let http = index(
            &[("minecraft", "1.21.4")],
            serde_json::json!([file("mods/a.jar", "http://cdn.modrinth.com/a.jar", "required")]),
        );
        assert!(download_jobs(&http, Path::new("game")).is_err());
    }

    fn write_pack(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[tokio::test]
    async fn installs_overrides_and_cleans_up_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path().join("data")));
        let modrinth = Modrinth::new();
        let index = br#"{"formatVersion":1,"game":"minecraft","versionId":"1","name":"Cozy Pack",
            "files":[],"dependencies":{"minecraft":"1.21.4"}}"#;

        let pack = tmp.path().join("cozy.mrpack");
        write_pack(
            &pack,
            &[
                (INDEX, index),
                ("overrides/options.txt", b"fov:90"),
                ("overrides/config/a.toml", b"x=1"),
                ("client-overrides/options.txt", b"fov:100"),
            ],
        );
        let instance = install_pack(&store, &modrinth, &pack, &Progress::none()).await.unwrap();
        assert_eq!(instance.name, "Cozy Pack");
        let game_dir = store.game_dir(&instance).unwrap();
        assert_eq!(std::fs::read_to_string(game_dir.join("options.txt")).unwrap(), "fov:100");
        assert!(game_dir.join("config").join("a.toml").is_file());

        let evil = tmp.path().join("evil.mrpack");
        write_pack(&evil, &[(INDEX, index), ("overrides/../../escaped.txt", b"x")]);
        assert!(install_pack(&store, &modrinth, &evil, &Progress::none()).await.is_err());
        assert!(!tmp.path().join("data").join("instances").join("escaped.txt").exists());
        // Only the first pack's instance is left.
        assert_eq!(store.list().await.unwrap().len(), 1);

        let not_a_pack = tmp.path().join("x.mrpack");
        std::fs::write(&not_a_pack, b"hello").unwrap();
        let err = install_pack(&store, &modrinth, &not_a_pack, &Progress::none()).await.unwrap_err();
        assert!(err.to_string().contains("isn't a Modrinth modpack"));
    }
}
