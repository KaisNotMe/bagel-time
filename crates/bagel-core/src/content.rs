//! Content inside an instance: mods, resource packs and shader packs.
//!
//! Each kind lives in its own folder (`mods`, `resourcepacks`,
//! `shaderpacks`). The folder itself is the source of truth: any `.jar` (mods)
//! or `.zip` (packs) in it counts, and a `.disabled` suffix turns one off.
//! Next to `instance.json`, a manifest per kind (`mods.json`,
//! `resourcepacks.json`, `shaderpacks.json`) remembers where each file came
//! from (Modrinth project and version, title, icon) so the UI can show names
//! and check for updates.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::{DownloadJob, sha1_file};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::instance::{Instance, InstanceStore};
use crate::loaders::{GameVersion, Loader};
use crate::modrinth::{Modrinth, PlannedMod, ProjectType, Target, mod_loaders};
use crate::progress::Progress;

const DISABLED: &str = ".disabled";

/// Installs, toggles and removals all rewrite a manifest; one at a time.
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Mod,
    ResourcePack,
    Shader,
}

impl ContentKind {
    pub const ALL: [ContentKind; 3] = [ContentKind::Mod, ContentKind::ResourcePack, ContentKind::Shader];

    pub fn folder(self) -> &'static str {
        match self {
            ContentKind::Mod => "mods",
            ContentKind::ResourcePack => "resourcepacks",
            ContentKind::Shader => "shaderpacks",
        }
    }

    fn manifest(self) -> &'static str {
        match self {
            ContentKind::Mod => "mods.json",
            ContentKind::ResourcePack => "resourcepacks.json",
            ContentKind::Shader => "shaderpacks.json",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            ContentKind::Mod => ".jar",
            ContentKind::ResourcePack | ContentKind::Shader => ".zip",
        }
    }

    pub fn project_type(self) -> ProjectType {
        match self {
            ContentKind::Mod => ProjectType::Mod,
            ContentKind::ResourcePack => ProjectType::ResourcePack,
            ContentKind::Shader => ProjectType::Shader,
        }
    }

    pub fn from_project_type(t: &str) -> Option<Self> {
        match t {
            "mod" => Some(ContentKind::Mod),
            "resourcepack" => Some(ContentKind::ResourcePack),
            "shader" => Some(ContentKind::Shader),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ContentKind::Mod => "mod",
            ContentKind::ResourcePack => "resource pack",
            ContentKind::Shader => "shader",
        }
    }

    /// Whether an instance with this loader can use the kind at all.
    pub fn supported_by(self, loader: Loader) -> bool {
        match self {
            ContentKind::Mod | ContentKind::Shader => loader != Loader::Vanilla,
            ContentKind::ResourcePack => true,
        }
    }

    /// What a project of this kind has to fit in an instance.
    pub fn target(self, game: &GameVersion) -> Target {
        let (loaders, label) = match self {
            ContentKind::Mod => (
                mod_loaders(game.loader).to_vec(),
                format!("{} {}", game.loader, game.minecraft),
            ),
            ContentKind::ResourcePack => (vec!["minecraft"], format!("Minecraft {}", game.minecraft)),
            // Iris runs both Iris and OptiFine shader packs.
            ContentKind::Shader => (vec!["iris", "optifine"], format!("Iris on Minecraft {}", game.minecraft)),
        };
        Target {
            minecraft: game.minecraft.clone(),
            loaders,
            label,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(alias = "mods")]
    files: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
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

impl Entry {
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
pub struct InstalledContent {
    pub kind: ContentKind,
    /// File name without the `.disabled` suffix; identifies it in calls.
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
pub struct ContentUpdate {
    pub kind: ContentKind,
    pub file_name: String,
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
}

/// A file found in the content folder.
struct FoundFile {
    file_name: String,
    enabled: bool,
    size: u64,
}

pub struct InstanceContent {
    kind: ContentKind,
    dir: PathBuf,
    manifest: PathBuf,
    game: GameVersion,
}

impl InstanceContent {
    pub fn new(store: &InstanceStore, instance: &Instance, kind: ContentKind) -> Result<Self> {
        Ok(Self {
            kind,
            dir: store.game_dir(instance)?.join(kind.folder()),
            manifest: store.instance_dir(&instance.id)?.join(kind.manifest()),
            game: instance.game(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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

    fn matches_extension(&self, file_name: &str) -> bool {
        file_name.to_ascii_lowercase().ends_with(self.kind.extension())
    }

    async fn scan(&self) -> Result<Vec<FoundFile>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).at(&self.dir),
        };
        while let Some(entry) = entries.next_entry().await.at(&self.dir)? {
            let Some(name) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            let (file_name, enabled) = match name.strip_suffix(DISABLED) {
                Some(base) => (base.to_string(), false),
                None => (name, true),
            };
            if !self.matches_extension(&file_name) {
                continue;
            }
            let Ok(meta) = entry.metadata().await else { continue };
            if !meta.is_file() {
                continue;
            }
            out.push(FoundFile { file_name, enabled, size: meta.len() });
        }
        Ok(out)
    }

    /// Files in the folder, sorted by title.
    pub async fn list(&self) -> Result<Vec<InstalledContent>> {
        let files = self.scan().await?;
        let manifest = self.read_manifest().await?;
        let by_name: HashMap<&str, &Entry> =
            manifest.files.iter().map(|m| (m.file_name.as_str(), m)).collect();
        let mut out: Vec<InstalledContent> = files
            .into_iter()
            .map(|f| {
                let entry = by_name
                    .get(f.file_name.as_str())
                    .map(|e| (*e).clone())
                    .unwrap_or_else(|| Entry::unknown(&f.file_name));
                InstalledContent {
                    kind: self.kind,
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

    /// Modrinth project ids of the files present in the folder.
    pub async fn installed_projects(&self) -> Result<HashSet<String>> {
        Ok(self.list().await?.into_iter().filter_map(|m| m.project_id).collect())
    }

    /// Works out what to download for a project (and, for mods, its
    /// required dependencies).
    pub async fn plan(&self, modrinth: &Modrinth, project_id: &str, version_id: Option<&str>) -> Result<Vec<PlannedMod>> {
        if !self.kind.supported_by(self.game.loader) {
            return Err(Error::Mods(format!(
                "Vanilla instances can't use {}s. Create a Fabric or Quilt instance instead.",
                self.kind.label()
            )));
        }
        let installed = self.installed_projects().await?;
        let target = self.kind.target(&self.game);
        modrinth
            .plan_install(project_id, version_id, &target, self.kind == ContentKind::Mod, &installed)
            .await
    }

    /// Downloads planned files into the folder, replacing older files of the
    /// same projects.
    pub async fn install(&self, modrinth: &Modrinth, plan: &[PlannedMod], progress: &Progress) -> Result<()> {
        let _guard = LOCK.lock().await;
        let mut manifest = self.prune(self.read_manifest().await?).await?;

        let mut jobs = Vec::new();
        for planned in plan {
            let v = &planned.version;
            let file = v.primary_file().ok_or_else(|| {
                Error::Mods(format!("Version {} has no files to download.", v.version_number))
            })?;
            if !is_plain_file_name(&file.filename) {
                return Err(Error::Mods(format!("Refusing to save a file as \"{}\".", file.filename)));
            }
            jobs.push(DownloadJob {
                url: file.url.clone(),
                path: self.dir.join(&file.filename),
                sha1: Some(file.hashes.sha1.clone()),
                size: Some(file.size),
            });
        }
        tokio::fs::create_dir_all(&self.dir).await.at(&self.dir)?;
        modrinth.downloader().fetch_all("Downloading", jobs, progress).await?;

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
            let mut kept = Vec::with_capacity(manifest.files.len());
            for entry in manifest.files.drain(..) {
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
            manifest.files = kept;
            // A disabled copy of the same file would otherwise linger.
            remove_if_exists(&self.dir.join(format!("{}{DISABLED}", file.filename))).await?;

            let project = projects.get(&v.project_id);
            manifest.files.push(Entry {
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
        self.check_file_name(file_name)?;
        let _guard = LOCK.lock().await;
        let on = self.dir.join(file_name);
        let off = self.dir.join(format!("{file_name}{DISABLED}"));
        let (from, to) = if enabled { (off, on) } else { (on, off) };
        if tokio::fs::try_exists(&from).await.unwrap_or(false) {
            tokio::fs::rename(&from, &to).await.at(&to)?;
        }
        Ok(())
    }

    pub async fn remove(&self, file_name: &str) -> Result<()> {
        self.check_file_name(file_name)?;
        let _guard = LOCK.lock().await;
        self.delete_files(file_name).await?;
        let mut manifest = self.read_manifest().await?;
        manifest.files.retain(|m| m.file_name != file_name);
        self.write_manifest(&manifest).await
    }

    fn check_file_name(&self, name: &str) -> Result<()> {
        if is_plain_file_name(name) && self.matches_extension(name) {
            Ok(())
        } else {
            Err(Error::Mods(format!("\"{name}\" isn't a {} file.", self.kind.label())))
        }
    }

    async fn delete_files(&self, file_name: &str) -> Result<()> {
        remove_if_exists(&self.dir.join(file_name)).await?;
        remove_if_exists(&self.dir.join(format!("{file_name}{DISABLED}"))).await
    }

    /// Drops manifest entries whose files are gone.
    async fn prune(&self, mut manifest: Manifest) -> Result<Manifest> {
        let present: HashSet<String> = self.scan().await?.into_iter().map(|f| f.file_name).collect();
        manifest.files.retain(|m| present.contains(&m.file_name));
        Ok(manifest)
    }

    /// Hashes files that haven't been looked at yet (e.g. dropped in by hand
    /// or from a modpack) and asks Modrinth what they are. Returns true if
    /// anything new was learned.
    pub async fn identify(&self, modrinth: &Modrinth) -> Result<bool> {
        let _guard = LOCK.lock().await;
        let mut manifest = self.prune(self.read_manifest().await?).await?;
        let known: HashSet<String> = manifest
            .files
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
                self.dir.join(&f.file_name)
            } else {
                self.dir.join(format!("{}{DISABLED}", f.file_name))
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
            manifest.files.retain(|m| m.file_name != file_name);
            let mut entry = Entry::unknown(&file_name);
            if let Some(v) = versions.get(&sha1) {
                if let Some(p) = projects.get(&v.project_id) {
                    entry.title = p.title.clone();
                    entry.icon_url = p.icon_url.clone();
                }
                entry.project_id = Some(v.project_id.clone());
                entry.version_id = Some(v.id.clone());
                entry.version_number = Some(v.version_number.clone());
            }
            entry.sha1 = Some(sha1);
            manifest.files.push(entry);
        }
        self.write_manifest(&manifest).await?;
        Ok(true)
    }

    /// Files from Modrinth that have a newer version for this instance.
    pub async fn check_updates(&self, modrinth: &Modrinth) -> Result<Vec<ContentUpdate>> {
        let present: HashSet<String> = self.scan().await?.into_iter().map(|f| f.file_name).collect();
        let manifest = self.read_manifest().await?;
        let tracked: Vec<&Entry> = manifest
            .files
            .iter()
            .filter(|m| present.contains(&m.file_name) && m.project_id.is_some() && m.sha1.is_some())
            .collect();
        let hashes: Vec<String> = tracked.iter().filter_map(|m| m.sha1.clone()).collect();
        let latest = modrinth.latest_by_sha1(&hashes, &self.kind.target(&self.game)).await?;
        Ok(tracked
            .into_iter()
            .filter_map(|m| {
                let v = latest.get(m.sha1.as_ref()?)?;
                (m.version_id.as_ref() != Some(&v.id) && m.project_id.as_ref() == Some(&v.project_id)).then(|| {
                    ContentUpdate {
                        kind: self.kind,
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

async fn remove_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e).at(path),
        _ => Ok(()),
    }
}

/// A bare file name: nothing that could point outside the folder.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.starts_with('.')
}

/// "sodium-fabric-0.6.0+mc1.21.4.jar" -> "sodium-fabric-0.6.0+mc1.21.4".
fn title_from_file(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(dot) if dot > 0 => file_name[..dot].to_string(),
        _ => file_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::{GameVersion, Loader};
    use crate::paths::Paths;

    async fn setup(kind: ContentKind) -> (tempfile::TempDir, InstanceContent) {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path()));
        let game = GameVersion {
            minecraft: "1.21.4".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.16.10".into()),
        };
        let instance = store.create("Modded", game).await.unwrap();
        let content = InstanceContent::new(&store, &instance, kind).unwrap();
        std::fs::create_dir_all(content.dir()).unwrap();
        (tmp, content)
    }

    #[tokio::test]
    async fn lists_toggles_and_removes() {
        let (_tmp, mods) = setup(ContentKind::Mod).await;
        let dir = mods.dir().to_path_buf();
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
    async fn resource_packs_are_zips_in_their_own_folder() {
        let (_tmp, packs) = setup(ContentKind::ResourcePack).await;
        assert!(packs.dir().ends_with("resourcepacks"));
        std::fs::write(packs.dir().join("Faithful 32x.zip"), b"zip").unwrap();
        std::fs::write(packs.dir().join("not-a-pack.jar"), b"jar").unwrap();
        let list = packs.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Faithful 32x");
        assert_eq!(list[0].kind, ContentKind::ResourcePack);
        assert!(packs.remove("x.jar").await.is_err());
    }

    #[tokio::test]
    async fn rejects_paths_outside_the_folder() {
        let (_tmp, mods) = setup(ContentKind::Mod).await;
        for bad in ["../instance.json", "..", "a/b.jar", "C:evil.jar", "x.txt"] {
            assert!(mods.remove(bad).await.is_err(), "{bad}");
            assert!(mods.set_enabled(bad, false).await.is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_old_mods_manifest() {
        let m: Manifest = serde_json::from_str(r#"{"mods":[{"fileName":"a.jar","title":"A"}]}"#).unwrap();
        assert_eq!(m.files[0].file_name, "a.jar");
    }

    #[test]
    fn titles() {
        assert_eq!(title_from_file("lithium-0.14.jar"), "lithium-0.14");
        assert_eq!(title_from_file("Thing.JAR"), "Thing");
        assert_eq!(title_from_file("Pack v2.zip"), "Pack v2");
    }

    #[test]
    fn vanilla_supports_only_resource_packs() {
        assert!(ContentKind::ResourcePack.supported_by(Loader::Vanilla));
        assert!(!ContentKind::Mod.supported_by(Loader::Vanilla));
        assert!(!ContentKind::Shader.supported_by(Loader::Vanilla));
        let t = ContentKind::Shader.target(&GameVersion::vanilla("1.21.4"));
        assert_eq!(t.loaders, ["iris", "optifine"]);
    }
}
