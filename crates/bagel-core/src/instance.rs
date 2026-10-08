//! Instances: separate game folders, each with its own version and settings.
//!
//! Layout: `instances/<id>/instance.json` plus `instances/<id>/minecraft/`
//! (saves, mods, options).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoContext, Result, parse_json};
use crate::loaders::{GameVersion, Loader};
use crate::paths::Paths;

const META_FILE: &str = "instance.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    /// Folder name; also the stable identifier the UI uses.
    pub id: String,
    pub name: String,
    pub game_version: String,
    #[serde(default)]
    pub loader: Loader,
    /// Pinned so the instance doesn't change under the player's mods.
    #[serde(default)]
    pub loader_version: Option<String>,
    /// `None` means "use the global default".
    #[serde(default)]
    pub memory_mb: Option<u32>,
    /// Extra JVM arguments, space separated.
    #[serde(default)]
    pub java_args: Option<String>,
    /// Icon file name inside the instance folder, e.g. `icon.png`.
    #[serde(default)]
    pub icon: Option<String>,
    /// Unix seconds.
    pub created: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
}

impl Instance {
    pub fn game(&self) -> GameVersion {
        GameVersion {
            minecraft: self.game_version.clone(),
            loader: self.loader,
            loader_version: self.loader_version.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstanceStore {
    dir: PathBuf,
}

impl InstanceStore {
    pub fn new(paths: &Paths) -> Self {
        Self {
            dir: paths.instances_dir(),
        }
    }

    pub fn instance_dir(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_id(id) {
            return Err(Error::InstanceNotFound(id.to_string()));
        }
        Ok(self.dir.join(id))
    }

    pub fn game_dir(&self, instance: &Instance) -> Result<PathBuf> {
        Ok(self.instance_dir(&instance.id)?.join("minecraft"))
    }

    /// All instances, most recently played first. Folders without a readable
    /// `instance.json` are ignored.
    pub async fn list(&self) -> Result<Vec<Instance>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e).at(&self.dir),
        };
        while let Some(entry) = entries.next_entry().await.at(&self.dir)? {
            let Some(id) = entry.file_name().to_str().map(String::from) else {
                continue;
            };
            if let Ok(instance) = self.get(&id).await {
                out.push(instance);
            }
        }
        out.sort_by(|a, b| {
            b.last_played
                .unwrap_or(b.created)
                .cmp(&a.last_played.unwrap_or(a.created))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    pub async fn get(&self, id: &str) -> Result<Instance> {
        let path = self.instance_dir(id)?.join(META_FILE);
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::InstanceNotFound(id.to_string()));
            }
            Err(e) => return Err(e).at(&path),
        };
        let mut instance: Instance = parse_json(&bytes, &path.display().to_string())?;
        // The folder name is the source of truth for the id.
        instance.id = id.to_string();
        Ok(instance)
    }

    /// `game.loader_version` must already be resolved for modded instances.
    pub async fn create(&self, name: &str, game: GameVersion) -> Result<Instance> {
        let name = name.trim();
        let id = self.unique_id(name).await;
        let instance = Instance {
            id,
            name: if name.is_empty() { game.minecraft.clone() } else { name.to_string() },
            game_version: game.minecraft,
            loader: game.loader,
            loader_version: game.loader_version.filter(|_| game.loader != Loader::Vanilla),
            memory_mb: None,
            java_args: None,
            icon: None,
            created: now(),
            last_played: None,
        };
        let game_dir = self.game_dir(&instance)?;
        tokio::fs::create_dir_all(&game_dir).await.at(&game_dir)?;
        self.save(&instance).await?;
        Ok(instance)
    }

    /// A free folder name based on `name`.
    async fn unique_id(&self, name: &str) -> String {
        let base = slugify(name);
        let mut id = base.clone();
        let mut n = 2;
        while tokio::fs::try_exists(self.dir.join(&id)).await.unwrap_or(false) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Copies an instance (worlds, mods and settings included) under a new name.
    pub async fn duplicate(&self, id: &str, name: &str) -> Result<Instance> {
        let mut copy = self.get(id).await?;
        let name = name.trim();
        copy.name = if name.is_empty() { format!("{} (copy)", copy.name) } else { name.to_string() };
        copy.id = self.unique_id(&copy.name).await;
        copy.created = now();
        copy.last_played = None;
        let (from, to) = (self.instance_dir(id)?, self.instance_dir(&copy.id)?);
        let dest = to.clone();
        tokio::task::spawn_blocking(move || copy_dir(&from, &dest)).await??;
        if let Err(e) = self.save(&copy).await {
            let _ = tokio::fs::remove_dir_all(&to).await;
            return Err(e);
        }
        Ok(copy)
    }

    pub fn icon_path(&self, instance: &Instance) -> Option<PathBuf> {
        let icon = instance.icon.as_deref()?;
        let dir = self.instance_dir(&instance.id).ok()?;
        Some(dir.join(icon))
    }

    /// Stores image bytes as the instance icon. `extension` is e.g. "png".
    pub async fn set_icon(&self, id: &str, bytes: &[u8], extension: &str) -> Result<Instance> {
        let ext = extension.to_ascii_lowercase();
        if !ICON_EXTENSIONS.contains(&ext.as_str()) {
            return Err(Error::Invalid(format!("Icons can be PNG, JPEG, WebP or GIF images, not .{ext}.")));
        }
        let mut instance = self.get(id).await?;
        let dir = self.instance_dir(id)?;
        if let Some(old) = self.icon_path(&instance) {
            let _ = tokio::fs::remove_file(old).await;
        }
        let file = format!("icon.{ext}");
        let path = dir.join(&file);
        tokio::fs::write(&path, bytes).await.at(&path)?;
        instance.icon = Some(file);
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn set_icon_from_file(&self, id: &str, source: &Path) -> Result<Instance> {
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or_default();
        let bytes = tokio::fs::read(source).await.at(source)?;
        self.set_icon(id, &bytes, ext).await
    }

    pub async fn clear_icon(&self, id: &str) -> Result<Instance> {
        let mut instance = self.get(id).await?;
        if let Some(old) = self.icon_path(&instance) {
            let _ = tokio::fs::remove_file(old).await;
        }
        instance.icon = None;
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn save(&self, instance: &Instance) -> Result<()> {
        let dir = self.instance_dir(&instance.id)?;
        tokio::fs::create_dir_all(&dir).await.at(&dir)?;
        let path = dir.join(META_FILE);
        let tmp = dir.join(format!("{META_FILE}.tmp"));
        let json = serde_json::to_vec_pretty(instance).expect("instance serializes");
        tokio::fs::write(&tmp, json).await.at(&tmp)?;
        tokio::fs::rename(&tmp, &path).await.at(&path)
    }

    pub async fn mark_played(&self, id: &str) -> Result<Instance> {
        let mut instance = self.get(id).await?;
        instance.last_played = Some(now());
        self.save(&instance).await?;
        Ok(instance)
    }

    /// Permanently removes the instance folder, including its worlds.
    pub async fn delete(&self, id: &str) -> Result<()> {
        // Make sure it's really an instance before deleting anything.
        self.get(id).await?;
        let dir = self.instance_dir(id)?;
        tokio::fs::remove_dir_all(&dir).await.at(&dir)
    }
}

const ICON_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "gif"];

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).at(to)?;
    for entry in std::fs::read_dir(from).at(from)? {
        let entry = entry.at(from)?;
        let (src, dest) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().at(&src)?;
        if kind.is_dir() {
            copy_dir(&src, &dest)?;
        } else if kind.is_file() {
            std::fs::copy(&src, &dest).at(&dest)?;
        }
    }
    Ok(())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Ids are slugs, which also keeps them from escaping the instances folder.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.starts_with('-')
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(40).collect();
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() { "instance".to_string() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("My Cool Pack!"), "my-cool-pack");
        assert_eq!(slugify("  1.21.4  "), "1-21-4");
        assert_eq!(slugify("???"), "instance");
        assert!(is_valid_id(&slugify("Ünïcode name")));
    }

    #[test]
    fn rejects_path_tricks() {
        assert!(!is_valid_id(".."));
        assert!(!is_valid_id("a/b"));
        assert!(!is_valid_id("a\\b"));
        assert!(!is_valid_id(""));
    }

    #[tokio::test]
    async fn create_list_play_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path()));

        let a = store.create("Survival", GameVersion::vanilla("1.21")).await.unwrap();
        let b = store
            .create(
                "Survival",
                GameVersion {
                    minecraft: "1.20.1".into(),
                    loader: Loader::Fabric,
                    loader_version: Some("0.16.10".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(a.id, "survival");
        assert_eq!(b.id, "survival-2");
        assert!(store.game_dir(&a).unwrap().is_dir());
        let reloaded = store.get("survival-2").await.unwrap();
        assert_eq!(reloaded.game().loader, Loader::Fabric);
        assert_eq!(reloaded.loader_version.as_deref(), Some("0.16.10"));

        store.mark_played("survival").await.unwrap();
        let ids: Vec<_> = store.list().await.unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(ids[0], "survival");
        assert_eq!(ids.len(), 2);

        store.delete("survival-2").await.unwrap();
        assert_eq!(store.list().await.unwrap().len(), 1);
        assert!(matches!(store.get("survival-2").await, Err(Error::InstanceNotFound(_))));
    }

    #[tokio::test]
    async fn duplicate_copies_everything_and_icons_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(&Paths::new(tmp.path()));
        let a = store.create("Pack", GameVersion::vanilla("1.21")).await.unwrap();
        let saves = store.game_dir(&a).unwrap().join("saves").join("World");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("level.dat"), b"x").unwrap();
        store.set_icon("pack", b"png", "PNG").await.unwrap();
        assert!(store.set_icon("pack", b"x", "exe").await.is_err());

        let b = store.duplicate("pack", "").await.unwrap();
        assert_eq!(b.id, "pack-copy");
        assert_eq!(b.name, "Pack (copy)");
        assert!(b.last_played.is_none());
        let copied = store.game_dir(&b).unwrap().join("saves").join("World").join("level.dat");
        assert!(copied.is_file());
        assert_eq!(std::fs::read(store.icon_path(&b).unwrap()).unwrap(), b"png");

        let cleared = store.clear_icon("pack").await.unwrap();
        assert!(cleared.icon.is_none());
        assert!(!store.instance_dir("pack").unwrap().join("icon.png").exists());
    }

    #[tokio::test]
    async fn ignores_folders_without_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path());
        std::fs::create_dir_all(paths.instances_dir().join("random")).unwrap();
        assert!(InstanceStore::new(&paths).list().await.unwrap().is_empty());
    }
}
