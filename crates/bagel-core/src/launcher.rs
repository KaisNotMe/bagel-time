use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::assets::install_assets;
use crate::download::{DownloadJob, Downloader};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::java::{LEGACY_COMPONENT, ensure_runtime};
use crate::launch::{LaunchContext, build_arguments};
use crate::libraries::{extract_natives, resolve};
use crate::meta::{VERSION_MANIFEST_URL, VersionJson, VersionManifest};
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::Environment;

pub struct LaunchOptions {
    pub account: Account,
    /// Defaults to `instances/<version id>`.
    pub game_dir: Option<PathBuf>,
    pub memory_mb: u32,
}

/// Everything needed to start a version, after its files are downloaded.
#[derive(Debug)]
pub struct InstalledVersion {
    pub version: VersionJson,
    pub java: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub game_assets: PathBuf,
    pub log_config: Option<PathBuf>,
}

pub struct Launcher {
    paths: Paths,
    dl: Downloader,
    env: Environment,
}

impl Launcher {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            dl: Downloader::new(),
            env: Environment::current(),
        }
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn default_game_dir(&self, version_id: &str) -> PathBuf {
        self.paths.instances_dir().join(version_id)
    }

    /// Fetch the version list, falling back to the last cached copy when offline.
    pub async fn version_manifest(&self) -> Result<VersionManifest> {
        let cache = self.paths.version_manifest();
        match self.dl.get_bytes(VERSION_MANIFEST_URL).await {
            Ok(bytes) => {
                let manifest = parse_json(&bytes, "version manifest")?;
                if let Some(parent) = cache.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let _ = tokio::fs::write(&cache, &bytes).await;
                Ok(manifest)
            }
            Err(e) => match tokio::fs::read(&cache).await {
                Ok(bytes) => parse_json(&bytes, "cached version manifest"),
                Err(_) => Err(e),
            },
        }
    }

    async fn version_json(&self, id: &str) -> Result<VersionJson> {
        let path = self.paths.version_json(id);
        if !path.exists() {
            let manifest = self.version_manifest().await?;
            let entry = manifest
                .versions
                .iter()
                .find(|v| v.id == id)
                .ok_or_else(|| Error::UnknownVersion(id.to_string()))?;
            self.dl
                .fetch(&DownloadJob {
                    url: entry.url.clone(),
                    path: path.clone(),
                    sha1: Some(entry.sha1.clone()),
                    size: None,
                })
                .await?;
        }
        let bytes = tokio::fs::read(&path).await.at(&path)?;
        parse_json(&bytes, &format!("version {id}"))
    }

    /// Download everything a version needs: client jar, libraries, assets and Java.
    /// Files already on disk are skipped, so this is cheap to call before every launch.
    pub async fn install(&self, id: &str, game_dir: &Path, progress: &Progress) -> Result<InstalledVersion> {
        progress.stage("Reading version info", 0);
        let version = self.version_json(id).await?;

        let libs = resolve(&version.libraries, &self.paths.libraries_dir(), &self.env);
        let mut jobs = libs.jobs;
        let jar = self.paths.version_jar(id);
        if let Some(client) = version.downloads.get("client") {
            jobs.push(DownloadJob {
                url: client.url.clone(),
                path: jar.clone(),
                sha1: client.sha1.clone(),
                size: client.size,
            });
        }
        let log_config = version.logging.as_ref().and_then(|l| l.client.as_ref()).map(|c| {
            let path = self.paths.log_config(&c.file.id);
            jobs.push(DownloadJob {
                url: c.file.url.clone(),
                path: path.clone(),
                sha1: Some(c.file.sha1.clone()),
                size: Some(c.file.size),
            });
            path
        });
        self.dl.fetch_all("Downloading libraries", jobs, progress).await?;

        let game_assets = install_assets(&self.dl, &self.paths, &version.asset_index, game_dir, progress).await?;

        let component = version
            .java_version
            .as_ref()
            .map_or(LEGACY_COMPONENT, |j| j.component.as_str());
        let java = ensure_runtime(&self.dl, &self.paths, &self.env, component, progress).await?;

        progress.stage("Extracting natives", 0);
        let natives_dir = self.paths.natives_dir(id);
        extract_natives(libs.natives, natives_dir.clone()).await?;

        let mut classpath = libs.classpath;
        classpath.push(jar);
        Ok(InstalledVersion {
            version,
            java,
            classpath,
            natives_dir,
            game_assets,
            log_config,
        })
    }

    /// Install if needed, then return a ready-to-spawn command. The caller decides
    /// what to do with the game's output.
    pub async fn prepare_launch(
        &self,
        id: &str,
        options: &LaunchOptions,
        progress: &Progress,
    ) -> Result<tokio::process::Command> {
        let game_dir = options
            .game_dir
            .clone()
            .unwrap_or_else(|| self.default_game_dir(id));
        tokio::fs::create_dir_all(&game_dir).await.at(&game_dir)?;

        let installed = self.install(id, &game_dir, progress).await?;
        let args = build_arguments(&LaunchContext {
            version: &installed.version,
            account: &options.account,
            env: &self.env,
            game_dir: &game_dir,
            assets_root: &self.paths.assets_dir(),
            game_assets: &installed.game_assets,
            libraries_dir: &self.paths.libraries_dir(),
            natives_dir: &installed.natives_dir,
            classpath: &installed.classpath,
            log_config: installed.log_config.as_deref(),
            memory_mb: options.memory_mb,
        });

        let mut cmd = tokio::process::Command::new(&installed.java);
        cmd.args(args).current_dir(&game_dir);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        Ok(cmd)
    }
}
