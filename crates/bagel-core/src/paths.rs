use std::path::{Path, PathBuf};

/// On-disk layout of the launcher's data folder.
///
/// Libraries, assets and Java runtimes are shared between instances so each
/// instance only stores its own saves, mods and configs.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%APPDATA%\BagelTime\data` on Windows, the platform equivalent elsewhere.
    pub fn default_location() -> Option<Self> {
        directories::ProjectDirs::from("", "", "BagelTime").map(|d| Self::new(d.data_dir()))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn version_manifest(&self) -> PathBuf {
        self.versions_dir().join("version_manifest_v2.json")
    }

    pub fn version_json(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id).join(format!("{id}.json"))
    }

    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id).join(format!("{id}.jar"))
    }

    /// e.g. `versions/fabric-0.16.10-1.21.4/profile.json`.
    pub fn loader_profile(&self, loader: &str, minecraft: &str, loader_version: &str) -> PathBuf {
        self.versions_dir()
            .join(format!("{loader}-{loader_version}-{minecraft}"))
            .join("profile.json")
    }

    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.versions_dir().join(id).join("natives")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn asset_index(&self, id: &str) -> PathBuf {
        self.assets_dir().join("indexes").join(format!("{id}.json"))
    }

    pub fn asset_object(&self, hash: &str) -> PathBuf {
        self.assets_dir().join("objects").join(&hash[..2]).join(hash)
    }

    pub fn virtual_assets_dir(&self, index_id: &str) -> PathBuf {
        self.assets_dir().join("virtual").join(index_id)
    }

    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.assets_dir().join("log_configs").join(file_id)
    }

    pub fn java_dir(&self, component: &str) -> PathBuf {
        self.root.join("java").join(component)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }
}
