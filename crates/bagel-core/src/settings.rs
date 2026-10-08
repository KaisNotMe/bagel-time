use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};
use crate::paths::Paths;

/// Launcher-wide preferences, stored as `settings.json` in the data folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Name used until Microsoft login exists.
    pub offline_username: String,
    /// Default max memory for instances that don't set their own.
    pub memory_mb: u32,
    pub show_snapshots: bool,
    /// Extra JVM arguments for every instance, space separated.
    pub java_args: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            offline_username: "Player".to_string(),
            memory_mb: 4096,
            show_snapshots: false,
            java_args: String::new(),
        }
    }
}

impl Settings {
    /// Missing or unreadable settings fall back to defaults rather than
    /// stopping the launcher from opening.
    pub async fn load(paths: &Paths) -> Self {
        match tokio::fs::read(paths.settings_file()).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub async fn save(&self, paths: &Paths) -> Result<()> {
        let path = paths.settings_file();
        tokio::fs::create_dir_all(paths.root()).await.at(paths.root())?;
        let json = serde_json::to_vec_pretty(self).expect("settings serialize");
        tokio::fs::write(&path, json).await.at(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trip_and_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(tmp.path());
        assert_eq!(Settings::load(&paths).await.memory_mb, 4096);

        let s = Settings {
            offline_username: "Bagel".into(),
            memory_mb: 6144,
            show_snapshots: true,
            java_args: String::new(),
        };
        s.save(&paths).await.unwrap();
        let loaded = Settings::load(&paths).await;
        assert_eq!(loaded.offline_username, "Bagel");
        assert_eq!(loaded.memory_mb, 6144);

        // Unknown or missing fields don't break loading.
        std::fs::write(paths.settings_file(), r#"{"memoryMb": 2048, "future": 1}"#).unwrap();
        let loaded = Settings::load(&paths).await;
        assert_eq!(loaded.memory_mb, 2048);
        assert_eq!(loaded.offline_username, "Player");
    }
}
