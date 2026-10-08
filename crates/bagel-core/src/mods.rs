//! Mods inside an instance's `mods` folder.
//!
//! The folder itself is the source of truth: any `.jar` in it counts, and a
//! `.jar.disabled` suffix turns one off. Next to `instance.json`, `mods.json`
//! remembers where each file came from (Modrinth project and version, title,
//! icon) so the UI can show names and check for updates.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::download::{DownloadJob, sha1_file};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::instance::{Instance, InstanceStore};
use crate::loaders::GameVersion;
use crate::modrinth::{Modrinth, PlannedMod};
use crate::progress::Progress;

const MANIFEST: &str = "mods.json";
const DISABLED: &str = ".disabled";

/// Installs, toggles and removals all rewrite `mods.json`; one at a time.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    mods: Vec<ModEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModEntry {
    /// File name without the `.disabled` suffix.
    file_name: String,
    title: String,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    version_number: Option<String>,
    #[serde(default)]
    icon_url: Option<String>,
    /// Set once the file has been hashed (and looked up on Modrinth).
    #[serde(default)]
    sha1: Option<String>,
    /// Installed because another mod needed it.
    #[serde(default)]
    dependency: bool,
}

impl ModEntry {
    fn unknown(file_name: &str) -> Self {
        Self {
            file_name: file_name.to_string(),
            title: title_from_file(file_name),
            project_id: None,
            version_id: None,
            version_number: None,
            icon_url: None,
            sha1: None,
            dependency: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    /// File name without the `.disabled` suffix; identifies the mod in calls.
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    pub title: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub version_number: Option<String>,
    pub icon_url: Option<String>,
    pub dependency: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdate {
    pub file_name: String,
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
}

/// A jar found in the mods folder.
struct ModFile {
    file_name: String,
    enabled: bool,
    size: u64,
}

pub struct InstanceMods {
    mods_dir: PathBuf,
    manifest: PathBuf,
    game: GameVersion,
}

impl InstanceMods {
    pub fn new(store: &InstanceStore, instance: &Instance) -> Result<Self> {
        Ok(Self {
            mods_dir: store.game_dir(instance)?.join("mods"),
            manifest: store.instance_dir(&instance.id)?.join(MANIFEST),
            game: instance.game(),
        })
    }

    pub fn mods_dir(&self) -> &std::path::Path {
        &self.mods_dir
    }

    async fn read_manifest(&self) -> Result<Manifest> {
        match tokio::fs::read(&self.manifest).await {
            Ok(bytes) => parse_json(&bytes, &self.manifest.display().to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Manifest::default()),
            Err(e) => Err(e).at(&self.manifest),
        }
    }

    async fn write_manifest(&self, manifest: &Manifest) -> Result<()> {
        let tmp = self.manifest.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(manifest).expect("manifest serializes");
        tokio::fs::write(&tmp, json).await.at(&tmp)?;
        tokio::fs::rename(&tmp, &self.manifest).await.at(&self.manifest)
    }

    async fn scan(&self) -> Result<Vec<ModFile>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.mods_dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).at(&self.mods_dir),
        };
        while let Some(entry) = entries.next_entry().await.at(&self.mods_dir)? {
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let (file_name, enabled) = match name.strip_suffix(DISABLED) {
                Some(base) => (base.to_string(), false),
                None => (name, true),
            };
            if !file_name.to_ascii_lowercase().ends_with(".jar") {
                continue;
            }
            let Ok(meta) = entry.metadata().await else { continue };
            if !meta.is_file() {
                continue;
            }
            out.push(ModFile { file_name, enabled, size: meta.len() });
        }
        Ok(out)
    }

    /// Mods in the folder, sorted by title.
    pub async fn list(&self) -> Result<Vec<InstalledMod>> {
        let files = self.scan().await?;
        let manifest = self.read_manifest().await?;
        let by_name: HashMap<&str, &ModEntry> =
            manifest.mods.iter().map(|m| (m.file_name.as_str(), m)).collect();
        let mut out: Vec<InstalledMod> = files
            .into_iter()
            .map(|f| {
                let entry = by_name
                    .get(f.file_name.as_str())
                    .map(|e| (*e).clone())
                    .unwrap_or_else(|| ModEntry::unknown(&f.file_name));
                InstalledMod {
                    file_name: f.file_name,
                    enabled: f.enabled,
                    size: f.size,
                    title: entry.title,
                    project_id: entry.project_id,
                    version_id: entry.version_id,
                    version_number: entry.version_number,
                    icon_url: entry.icon_url,
                    dependency: entry.dependency,
                }
            })
            .collect();
        out.sort_by_key(|m| m.title.to_lowercase());
        Ok(out)
    }

    /// Modrinth project ids of the mods present in the folder.
    pub async fn installed_projects(&self) -> Result<HashSet<String>> {
        Ok(self.list().await?.into_iter().filter_map(|m| m.project_id).collect())
    }

    /// Downloads planned mods into the folder, replacing older files of the
    /// same projects.
    pub async fn install(&self, modrinth: &Modrinth, plan: &[PlannedMod], progress: &Progress) -> Result<()> {
        let _guard = LOCK.lock().await;
        let mut manifest = self.prune(self.read_manifest().await?).await?;

        let mut jobs = Vec::new();
        for planned in plan {
            let v = &planned.version;
            let file = v.primary_file().ok_or_else(|| {
                Error::Mods(format!("Version {} of this mod has no files to download.", v.version_number))
            })?;
            if !is_plain_file_name(&file.filename) {
                return Err(Error::Mods(format!("Refusing to save a mod as \"{}\".", file.filename)));
            }
            jobs.push(DownloadJob {
                url: file.url.clone(),
                path: self.mods_dir.join(&file.filename),
                sha1: Some(file.hashes.sha1.clone()),
                size: Some(file.size),
            });
        }
        tokio::fs::create_dir_all(&self.mods_dir).await.at(&self.mods_dir)?;
        modrinth.downloader().fetch_all("Downloading mods", jobs, progress).await?;

        let ids: Vec<String> = plan.iter().map(|p| p.version.project_id.clone()).collect();
        let projects: HashMap<String, _> = modrinth
            .projects(&ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();

        for planned in plan {
            let v = &planned.version;
            let file = v.primary_file().expect("checked above");
            // Remove older files of this project, keeping whether the user
            // installed it themselves.
            let mut dependency = planned.dependency;
            let mut kept = Vec::with_capacity(manifest.mods.len());
            for entry in manifest.mods.drain(..) {
                let same_project = entry.project_id.as_deref() == Some(v.project_id.as_str());
                if same_project || entry.file_name == file.filename {
                    dependency &= entry.dependency;
                    if entry.file_name != file.filename {
                        self.delete_files(&entry.file_name).await?;
                    }
                } else {
                    kept.push(entry);
                }
            }
            manifest.mods = kept;
            // A disabled copy of the same file would otherwise linger.
            remove_if_exists(&self.mods_dir.join(format!("{}{DISABLED}", file.filename))).await?;

            let project = projects.get(&v.project_id);
            manifest.mods.push(ModEntry {
                file_name: file.filename.clone(),
                title: project.map_or_else(|| title_from_file(&file.filename), |p| p.title.clone()),
                project_id: Some(v.project_id.clone()),
                version_id: Some(v.id.clone()),
                version_number: Some(v.version_number.clone()),
                icon_url: project.and_then(|p| p.icon_url.clone()),
                sha1: Some(file.hashes.sha1.clone()),
                dependency,
            });
        }
        self.write_manifest(&manifest).await
    }

    pub async fn set_enabled(&self, file_name: &str, enabled: bool) -> Result<()> {
        check_file_name(file_name)?;
        let _guard = LOCK.lock().await;
        let on = self.mods_dir.join(file_name);
        let off = self.mods_dir.join(format!("{file_name}{DISABLED}"));
        let (from, to) = if enabled { (off, on) } else { (on, off) };
        if tokio::fs::try_exists(&from).await.unwrap_or(false) {
            tokio::fs::rename(&from, &to).await.at(&to)?;
        }
        Ok(())
    }

    pub async fn remove(&self, file_name: &str) -> Result<()> {
        check_file_name(file_name)?;
        let _guard = LOCK.lock().await;
        self.delete_files(file_name).await?;
        let mut manifest = self.read_manifest().await?;
        manifest.mods.retain(|m| m.file_name != file_name);
        self.write_manifest(&manifest).await
    }

    async fn delete_files(&self, file_name: &str) -> Result<()> {
        remove_if_exists(&self.mods_dir.join(file_name)).await?;
        remove_if_exists(&self.mods_dir.join(format!("{file_name}{DISABLED}"))).await
    }

    /// Drops manifest entries whose files are gone.
    async fn prune(&self, mut manifest: Manifest) -> Result<Manifest> {
        let present: HashSet<String> = self.scan().await?.into_iter().map(|f| f.file_name).collect();
        manifest.mods.retain(|m| present.contains(&m.file_name));
        Ok(manifest)
    }

    /// Hashes jars that haven't been looked at yet (e.g. dropped in by hand
    /// or from a modpack) and asks Modrinth what they are. Returns true if
    /// anything new was learned.
    pub async fn identify(&self, modrinth: &Modrinth) -> Result<bool> {
        let _guard = LOCK.lock().await;
        let mut manifest = self.prune(self.read_manifest().await?).await?;
        let known: HashSet<String> = manifest
            .mods
            .iter()
            .filter(|m| m.sha1.is_some())
            .map(|m| m.file_name.clone())
            .collect();
        let mut hashed = Vec::new();
        for f in self.scan().await? {
            if known.contains(&f.file_name) {
                continue;
            }
            let path = if f.enabled {
                self.mods_dir.join(&f.file_name)
            } else {
                self.mods_dir.join(format!("{}{DISABLED}", f.file_name))
            };
            hashed.push((f.file_name, sha1_file(&path).await?));
        }
        if hashed.is_empty() {
            return Ok(false);
        }

        let hashes: Vec<String> = hashed.iter().map(|(_, h)| h.clone()).collect();
        let versions = modrinth.versions_by_sha1(&hashes).await?;
        let ids: Vec<String> = versions.values().map(|v| v.project_id.clone()).collect();
        let projects: HashMap<String, _> = modrinth
            .projects(&ids)
            .await?
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();

        for (file_name, sha1) in hashed {
            manifest.mods.retain(|m| m.file_name != file_name);
            let mut entry = ModEntry::unknown(&file_name);
            if let Some(v) = versions.get(&sha1) {
                let project = projects.get(&v.project_id);
                if let Some(p) = project {
                    entry.title = p.title.clone();
                    entry.icon_url = p.icon_url.clone();
                }
                entry.project_id = Some(v.project_id.clone());
                entry.version_id = Some(v.id.clone());
                entry.version_number = Some(v.version_number.clone());
            }
            entry.sha1 = Some(sha1);
            manifest.mods.push(entry);
        }
        self.write_manifest(&manifest).await?;
        Ok(true)
    }

    /// Mods from Modrinth that have a newer version for this instance.
    pub async fn check_updates(&self, modrinth: &Modrinth) -> Result<Vec<ModUpdate>> {
        let present: HashSet<String> = self.scan().await?.into_iter().map(|f| f.file_name).collect();
        let manifest = self.read_manifest().await?;
        let tracked: Vec<&ModEntry> = manifest
            .mods
            .iter()
            .filter(|m| present.contains(&m.file_name) && m.project_id.is_some() && m.sha1.is_some())
            .collect();
        let hashes: Vec<String> = tracked.iter().filter_map(|m| m.sha1.clone()).collect();
        let latest = modrinth.latest_by_sha1(&hashes, &self.game).await?;
        Ok(tracked
            .into_iter()
            .filter_map(|m| {
                let v = latest.get(m.sha1.as_ref()?)?;
                (m.version_id.as_ref() != Some(&v.id) && m.project_id.as_ref() == Some(&v.project_id)).then(|| {
                    ModUpdate {
                        file_name: m.file_name.clone(),
                        project_id: v.project_id.clone(),
                        version_id: v.id.clone(),
                        version_number: v.version_number.clone(),
                    }
                })
            })
            .collect())
    }
}

async fn remove_if_exists(path: &std::path::Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).at(path),
        _ => Ok(()),
    }
}

/// A bare file name: nothing that could point outside the mods folder.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.starts_with('.')
}

fn check_file_name(name: &str) -> Result<()> {
    if is_plain_file_name(name) && name.to_ascii_lowercase().ends_with(".jar") {
        Ok(())
    } else {
        Err(Error::Mods(format!("\"{name}\" isn't a mod file.")))
    }
}

/// "sodium-fabric-0.6.0+mc1.21.4.jar" -> "sodium-fabric-0.6.0+mc1.21.4".
fn title_from_file(file_name: &str) -> String {
    let lower = file_name.to_ascii_lowercase();
    match lower.strip_suffix(".jar") {
        Some(stem) => file_name[..stem.len()].to_string(),
        None => file_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::{GameVersion, Loader};
    use crate::paths::Paths;

    async fn setup() -> (tempfile::TempDir, InstanceMods) {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path()));
        let game = GameVersion {
            minecraft: "1.21.4".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.16.10".into()),
        };
        let instance = store.create("Modded", game).await.unwrap();
        let mods = InstanceMods::new(&store, &instance).unwrap();
        std::fs::create_dir_all(mods.mods_dir()).unwrap();
        (tmp, mods)
    }

    #[tokio::test]
    async fn lists_toggles_and_removes() {
        let (_tmp, mods) = setup().await;
        let dir = mods.mods_dir().to_path_buf();
        std::fs::write(dir.join("Sodium-1.0.jar"), b"abc").unwrap();
        std::fs::write(dir.join("old.jar.disabled"), b"x").unwrap();
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();

        let list = mods.list().await.unwrap();
        let names: Vec<_> = list.iter().map(|m| (m.title.as_str(), m.enabled, m.size)).collect();
        assert_eq!(names, [("old", false, 1), ("Sodium-1.0", true, 3)]);

        mods.set_enabled("Sodium-1.0.jar", false).await.unwrap();
        assert!(dir.join("Sodium-1.0.jar.disabled").exists());
        mods.set_enabled("old.jar", true).await.unwrap();
        assert!(dir.join("old.jar").exists());

        mods.remove("Sodium-1.0.jar").await.unwrap();
        assert!(!dir.join("Sodium-1.0.jar.disabled").exists());
        assert_eq!(mods.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rejects_paths_outside_the_folder() {
        let (_tmp, mods) = setup().await;
        for bad in ["../instance.json", "..", "a/b.jar", "C:evil.jar", "x.txt"] {
            assert!(mods.remove(bad).await.is_err(), "{bad}");
            assert!(mods.set_enabled(bad, false).await.is_err(), "{bad}");
        }
    }

    #[test]
    fn titles() {
        assert_eq!(title_from_file("lithium-0.14.jar"), "lithium-0.14");
        assert_eq!(title_from_file("Thing.JAR"), "Thing");
    }
}
