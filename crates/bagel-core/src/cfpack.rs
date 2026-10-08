//! CurseForge modpacks: a zip with `manifest.json` listing CurseForge files
//! by project and file id, plus an overrides folder copied into the game
//! folder.
//!
//! Some authors only allow downloads from the CurseForge website. Those files
//! can't be fetched here; they come back as [`ManualDownload`]s for the
//! player to get by hand.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::content::{ContentKind, InstanceContent, Sources};
use crate::curseforge::{self, CfFile, CfMod, CurseForge};
use crate::download::DownloadJob;
use crate::error::{Error, IoContext, Result, parse_json};
use crate::instance::{Instance, InstanceStore};
use crate::loaders::{GameVersion, Loader};
use crate::mrpack::{self, safe_relative_path};
use crate::paths::Paths;
use crate::progress::Progress;

const MANIFEST: &str = "manifest.json";
const WORLDS_CLASS: u32 = 17;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub minecraft: PackMinecraft,
    #[serde(default)]
    pub manifest_type: Option<String>,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub files: Vec<PackFile>,
    #[serde(default = "default_overrides")]
    pub overrides: String,
}

fn default_overrides() -> String {
    "overrides".into()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMinecraft {
    pub version: String,
    #[serde(default)]
    pub mod_loaders: Vec<PackLoader>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackLoader {
    /// e.g. "forge-47.3.0" or "fabric-0.16.10".
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PackFile {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    #[serde(default = "yes")]
    pub required: bool,
}

fn yes() -> bool {
    true
}

/// A file the pack needs that has to be downloaded from the website.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDownload {
    pub title: String,
    pub file_name: String,
    /// The file's page on curseforge.com.
    pub url: String,
    /// Folder inside the game folder it belongs in, e.g. "mods".
    pub folder: String,
}

/// A new instance from a modpack, and anything still missing from it.
#[derive(Debug, Clone)]
pub struct PackInstall {
    pub instance: Instance,
    pub manual: Vec<ManualDownload>,
}

/// Reads the manifest from a CurseForge pack zip.
pub async fn read_manifest(pack: &Path) -> Result<PackManifest> {
    let pack = pack.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&pack).at(&pack)?;
        let mut zip = zip::ZipArchive::new(file)
            .map_err(|_| Error::Pack("That file isn't a CurseForge modpack (.zip).".into()))?;
        let mut entry = zip
            .by_name(MANIFEST)
            .map_err(|_| Error::Pack("That file isn't a CurseForge modpack: manifest.json is missing.".into()))?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).at(&pack)?;
        parse_json(&bytes, MANIFEST)
    })
    .await?
}

/// The Minecraft version and loader a pack needs.
pub fn pack_game(m: &PackManifest) -> Result<GameVersion> {
    if m.manifest_type.as_deref().is_some_and(|t| t != "minecraftModpack") {
        return Err(Error::Pack("This CurseForge pack isn't a Minecraft modpack.".into()));
    }
    let minecraft = m.minecraft.version.trim().to_string();
    if minecraft.is_empty() {
        return Err(Error::Pack("The pack doesn't say which Minecraft version it needs.".into()));
    }
    let loader = m
        .minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or(m.minecraft.mod_loaders.first());
    let Some(loader) = loader else {
        return Ok(GameVersion::vanilla(&minecraft));
    };
    let (name, version) = loader
        .id
        .split_once('-')
        .ok_or_else(|| Error::Pack(format!("Unknown mod loader \"{}\".", loader.id)))?;
    let kind = match name.to_ascii_lowercase().as_str() {
        "forge" => Loader::Forge,
        "neoforge" => Loader::NeoForge,
        "fabric" => Loader::Fabric,
        "quilt" => Loader::Quilt,
        _ => return Err(Error::Pack(format!("Bagel Time doesn't support the \"{name}\" loader."))),
    };
    // Some packs write "neoforge-1.20.1-47.1.106".
    let version = version.strip_prefix(&format!("{minecraft}-")).unwrap_or(version);
    Ok(GameVersion {
        minecraft,
        loader: kind,
        loader_version: Some(version.to_string()),
    })
}

/// Which folder a project's files go in, by its CurseForge class. Worlds
/// are left out: they're zips that would need unpacking into `saves`.
fn folder_for(class_id: Option<u32>) -> Option<&'static str> {
    if class_id == Some(WORLDS_CLASS) {
        return None;
    }
    match class_id.and_then(curseforge::project_type) {
        Some(crate::modrinth::ProjectType::ResourcePack) => Some(ContentKind::ResourcePack.folder()),
        Some(crate::modrinth::ProjectType::Shader) => Some(ContentKind::Shader.folder()),
        _ => Some(ContentKind::Mod.folder()),
    }
}

fn allowed_url(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| curseforge::DOWNLOAD_HOSTS.contains(&h))
    })
}

/// Splits the pack's files into downloads and ones to get by hand.
fn download_jobs(
    files: &[CfFile],
    mods: &HashMap<u64, CfMod>,
    game_dir: &Path,
) -> Result<(Vec<DownloadJob>, Vec<ManualDownload>)> {
    let mut jobs = Vec::new();
    let mut manual = Vec::new();
    for f in files {
        let m = mods.get(&f.mod_id);
        let Some(folder) = folder_for(m.and_then(|m| m.class_id)) else {
            continue;
        };
        let rel = safe_relative_path(&f.file_name)
            .filter(|p| p.components().count() == 1)
            .ok_or_else(|| Error::Pack(format!("The pack has a file with an unsafe name: {}", f.file_name)))?;
        match &f.download_url {
            Some(url) if allowed_url(url) => jobs.push(DownloadJob {
                url: url.clone(),
                path: game_dir.join(folder).join(rel),
                sha1: f.sha1(),
                size: Some(f.file_length).filter(|s| *s > 0),
            }),
            Some(url) => return Err(Error::Pack(format!("{} would download from an unknown site: {url}", f.file_name))),
            None => {
                let page = m.and_then(|m| m.website_url());
                manual.push(ManualDownload {
                    title: m.map_or_else(|| f.display_name.clone(), |m| m.name.clone()),
                    file_name: f.file_name.clone(),
                    url: page.map_or_else(
                        || format!("https://www.curseforge.com/projects/{}", f.mod_id),
                        |p| format!("{}/files/{}", p.trim_end_matches('/'), f.id),
                    ),
                    folder: folder.to_string(),
                });
            }
        }
    }
    Ok((jobs, manual))
}

/// Creates a new instance from a CurseForge pack zip. The instance is
/// removed again if anything fails part-way.
pub async fn install_pack(
    store: &InstanceStore,
    sources: &Sources,
    pack: &Path,
    progress: &Progress,
) -> Result<PackInstall> {
    progress.stage("Reading modpack", 0);
    let manifest = read_manifest(pack).await?;
    let game = pack_game(&manifest)?;
    let overrides = format!("{}/", manifest.overrides.trim_matches('/'));
    if safe_relative_path(&overrides).is_none() {
        return Err(Error::Pack("The pack's overrides folder has an unsafe name.".into()));
    }

    let cf = &sources.curseforge;
    let wanted: Vec<&PackFile> = manifest.files.iter().filter(|f| f.required).collect();
    let (files, mods) = if wanted.is_empty() {
        (Vec::new(), HashMap::new())
    } else {
        progress.stage("Looking up mods", 0);
        let ids: Vec<u64> = wanted.iter().map(|f| f.file_id).collect();
        let files = cf.files(&ids).await?;
        let mod_ids: Vec<u64> = wanted.iter().map(|f| f.project_id).collect();
        let mods: HashMap<u64, CfMod> = cf.mods(&mod_ids).await?.into_iter().map(|m| (m.id, m)).collect();
        (files, mods)
    };
    // Check every path before creating anything.
    download_jobs(&files, &mods, Path::new("."))?;

    let instance = store.create(&manifest.name, game).await?;
    let filled = async {
        let game_dir = store.game_dir(&instance)?;
        let (jobs, manual) = download_jobs(&files, &mods, &game_dir)?;
        cf.downloader().fetch_all("Downloading mods", jobs, progress).await?;

        progress.stage("Unpacking files", 0);
        let (pack, dir) = (pack.to_path_buf(), game_dir.clone());
        tokio::task::spawn_blocking(move || mrpack::extract_overrides(&pack, &dir, &[&overrides])).await??;

        // Names and icons are nice to have; the pack works without them.
        progress.stage("Looking up mods", 0);
        for kind in ContentKind::ALL {
            let _ = InstanceContent::new(store, &instance, kind)?.identify(sources).await;
        }
        Ok::<_, Error>(manual)
    }
    .await;
    match filled {
        Ok(manual) => Ok(PackInstall { instance, manual }),
        Err(e) => {
            let _ = store.delete(&instance.id).await;
            Err(e)
        }
    }
}

/// Downloads a modpack from CurseForge (the given file, or the newest
/// release) and installs it as a new instance.
pub async fn install_from_curseforge(
    paths: &Paths,
    store: &InstanceStore,
    sources: &Sources,
    project_id: &str,
    file_id: Option<&str>,
    progress: &Progress,
) -> Result<PackInstall> {
    let cf = &sources.curseforge;
    progress.stage("Finding the latest version", 0);
    let m = cf.get_mod(project_id).await?;
    let file = match file_id {
        Some(id) => cf.file(project_id, id).await?,
        None => {
            let files = cf.project_files(project_id, None, None).await?;
            curseforge::pick_best(&files)
                .cloned()
                .ok_or_else(|| Error::Pack("This modpack has no files to download.".into()))?
        }
    };
    let url = file.download_url.clone().ok_or_else(|| {
        Error::Pack(format!("{}'s author only allows downloading it from the CurseForge website.", m.name))
    })?;
    if !allowed_url(&url) {
        return Err(Error::Pack(format!("The modpack would download from an unknown site: {url}")));
    }

    let tmp = paths.cache_dir().join(format!("curseforge-{}.zip", file.id));
    progress.stage("Downloading modpack", 0);
    cf.downloader()
        .fetch(&DownloadJob {
            url,
            path: tmp.clone(),
            sha1: file.sha1(),
            size: Some(file.file_length).filter(|s| *s > 0),
        })
        .await?;
    let result = install_pack(store, sources, &tmp, progress).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    let mut done = result?;

    if let Some(url) = m.logo.as_ref().map(|l| l.url.clone()).filter(|u| !u.is_empty())
        && let Ok(updated) = set_icon_from(cf, store, &done.instance, &url).await
    {
        done.instance = updated;
    }
    Ok(done)
}

async fn set_icon_from(cf: &CurseForge, store: &InstanceStore, instance: &Instance, url: &str) -> Result<Instance> {
    let ext = url
        .rsplit_once('.')
        .map(|(_, e)| e.to_string())
        .ok_or_else(|| Error::Invalid("icon has no extension".into()))?;
    let bytes = cf.downloader().get_bytes(url).await?;
    store.set_icon(&instance.id, &bytes, &ext).await
}

/// Which kind of modpack a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackFormat {
    Modrinth,
    CurseForge,
}

pub async fn detect(pack: &Path) -> Result<PackFormat> {
    let pack = pack.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&pack).at(&pack)?;
        let zip = zip::ZipArchive::new(file).map_err(|_| Error::Pack("That file isn't a modpack.".into()))?;
        if zip.index_for_name("modrinth.index.json").is_some() {
            Ok(PackFormat::Modrinth)
        } else if zip.index_for_name(MANIFEST).is_some() {
            Ok(PackFormat::CurseForge)
        } else {
            Err(Error::Pack(
                "That file isn't a modpack Bagel Time knows (.mrpack or a CurseForge .zip).".into(),
            ))
        }
    })
    .await?
}

/// Creates an instance from a `.mrpack` or CurseForge `.zip`.
pub async fn import_file(store: &InstanceStore, sources: &Sources, pack: &Path, progress: &Progress) -> Result<PackInstall> {
    match detect(pack).await? {
        PackFormat::Modrinth => Ok(PackInstall {
            instance: mrpack::install_pack(store, sources, pack, progress).await?,
            manual: Vec::new(),
        }),
        PackFormat::CurseForge => install_pack(store, sources, pack, progress).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn manifest(loaders: serde_json::Value) -> PackManifest {
        serde_json::from_value(serde_json::json!({
            "minecraft": {"version": "1.20.1", "modLoaders": loaders},
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "Pack",
            "files": [{"projectID": 1, "fileID": 2, "required": true}],
            "overrides": "overrides"
        }))
        .unwrap()
    }

    #[test]
    fn game_from_manifest() {
        let forge = pack_game(&manifest(serde_json::json!([{"id": "forge-47.3.0", "primary": true}]))).unwrap();
        assert_eq!((forge.loader, forge.loader_version.as_deref()), (Loader::Forge, Some("47.3.0")));
        let neo = pack_game(&manifest(serde_json::json!([{"id": "neoforge-1.20.1-47.1.106", "primary": true}]))).unwrap();
        assert_eq!((neo.loader, neo.loader_version.as_deref()), (Loader::NeoForge, Some("47.1.106")));
        let two = pack_game(&manifest(serde_json::json!([
            {"id": "forge-1", "primary": false},
            {"id": "fabric-0.16.10", "primary": true}
        ])))
        .unwrap();
        assert_eq!(two.loader, Loader::Fabric);
        assert_eq!(pack_game(&manifest(serde_json::json!([]))).unwrap().loader, Loader::Vanilla);
        assert!(pack_game(&manifest(serde_json::json!([{"id": "rift-1", "primary": true}]))).is_err());
    }

    fn cf_file(id: u64, mod_id: u64, name: &str, url: Option<&str>) -> CfFile {
        serde_json::from_value(serde_json::json!({
            "id": id, "modId": mod_id, "displayName": name, "fileName": name,
            "releaseType": 1, "fileDate": "2026-01-01T00:00:00Z", "fileLength": 3,
            "downloadUrl": url, "gameVersions": ["1.20.1"], "hashes": [{"value": "aa", "algo": 1}]
        }))
        .unwrap()
    }

    fn cf_mod(id: u64, class_id: u32) -> CfMod {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": format!("Mod {id}"), "slug": "m", "classId": class_id,
            "links": {"websiteUrl": format!("https://www.curseforge.com/minecraft/mc-mods/m{id}")}
        }))
        .unwrap()
    }

    #[test]
    fn jobs_by_folder_and_manual_downloads() {
        let files = [
            cf_file(10, 1, "a.jar", Some("https://edge.forgecdn.net/files/1/10/a.jar")),
            cf_file(11, 2, "pack.zip", Some("https://mediafilez.forgecdn.net/files/1/11/pack.zip")),
            cf_file(12, 3, "blocked.jar", None),
            cf_file(13, 4, "world.zip", Some("https://edge.forgecdn.net/files/1/13/world.zip")),
        ];
        let mods: HashMap<u64, CfMod> =
            [cf_mod(1, 6), cf_mod(2, 12), cf_mod(3, 6), cf_mod(4, WORLDS_CLASS)].into_iter().map(|m| (m.id, m)).collect();
        let (jobs, manual) = download_jobs(&files, &mods, Path::new("g")).unwrap();
        let paths: Vec<_> = jobs.iter().map(|j| j.path.clone()).collect();
        assert_eq!(paths, [Path::new("g/mods/a.jar"), Path::new("g/resourcepacks/pack.zip")]);
        assert_eq!(manual.len(), 1);
        assert_eq!(manual[0].url, "https://www.curseforge.com/minecraft/mc-mods/m3/files/12");
        assert_eq!(manual[0].folder, "mods");

        let evil = [cf_file(1, 1, "../x.jar", Some("https://edge.forgecdn.net/x.jar"))];
        assert!(download_jobs(&evil, &mods, Path::new("g")).is_err());
        let host = [cf_file(1, 1, "x.jar", Some("https://evil.example.com/x.jar"))];
        assert!(download_jobs(&host, &mods, Path::new("g")).is_err());
    }

    #[tokio::test]
    async fn imports_overrides_and_detects_format() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path().join("data")));
        let pack = tmp.path().join("pack.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file(MANIFEST, opts).unwrap();
        zip.write_all(br#"{"minecraft":{"version":"1.21.4","modLoaders":[]},"manifestType":"minecraftModpack",
            "name":"Cozy CF","files":[],"overrides":"stuff"}"#)
            .unwrap();
        zip.start_file("stuff/config/a.toml", opts).unwrap();
        zip.write_all(b"x=1").unwrap();
        zip.start_file("other/b.txt", opts).unwrap();
        zip.write_all(b"no").unwrap();
        zip.finish().unwrap();

        assert_eq!(detect(&pack).await.unwrap(), PackFormat::CurseForge);
        let done = import_file(&store, &Sources::default(), &pack, &Progress::none()).await.unwrap();
        assert_eq!(done.instance.name, "Cozy CF");
        let dir = store.game_dir(&done.instance).unwrap();
        assert!(dir.join("config").join("a.toml").is_file());
        assert!(!dir.join("b.txt").exists() && !dir.join("other").exists());
    }
}
