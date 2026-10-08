use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::assets::install_assets;
use crate::download::{DownloadJob, Downloader};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::java::{LEGACY_COMPONENT, ensure_runtime};
use crate::launch::{LaunchContext, build_arguments};
use crate::libraries::{extract_natives, resolve};
use crate::loaders::{self, GameVersion, Loader, LoaderVersion, profile_url};
use crate::meta::{LoaderProfile, VERSION_MANIFEST_URL, VersionJson, VersionManifest};
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::Environment;

pub struct LaunchOptions {
    pub account: Account,
    /// Folder the game runs in (saves, options, mods).
    pub game_dir: PathBuf,
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

    /// Fetch (or reuse the cached) loader profile for a game version.
    async fn loader_profile(&self, loader: Loader, minecraft: &str, loader_version: &str) -> Result<LoaderProfile> {
        let path = self.paths.loader_profile(loader.slug(), minecraft, loader_version);
        if !path.exists() {
            let url = profile_url(loader, minecraft, loader_version)
                .ok_or_else(|| Error::MissingLoaderVersion(loader.to_string()))?;
            self.dl
                .fetch(&DownloadJob {
                    url,
                    path: path.clone(),
                    sha1: None,
                    size: None,
                })
                .await?;
        }
        let bytes = tokio::fs::read(&path).await.at(&path)?;
        let parsed = parse_json(&bytes, &format!("{loader} {loader_version} profile"));
        if parsed.is_err() {
            // Don't keep a bad response around; the next launch refetches it.
            let _ = tokio::fs::remove_file(&path).await;
        }
        parsed
    }

    /// The version JSON to launch: vanilla, with the loader profile applied if any.
    async fn resolve_version(&self, game: &GameVersion) -> Result<VersionJson> {
        let vanilla = self.version_json(&game.minecraft).await?;
        match (game.loader, game.loader_version.as_deref()) {
            (Loader::Vanilla, _) => Ok(vanilla),
            (loader, Some(loader_version)) => {
                let profile = self.loader_profile(loader, &game.minecraft, loader_version).await?;
                Ok(vanilla.with_profile(profile))
            }
            (loader, None) => Err(Error::MissingLoaderVersion(loader.to_string())),
        }
    }

    /// Loader versions available for a Minecraft version, newest first.
    pub async fn loader_versions(&self, loader: Loader, minecraft: &str) -> Result<Vec<LoaderVersion>> {
        loaders::loader_versions(&self.dl, loader, minecraft).await
    }

    pub async fn latest_stable_loader(&self, loader: Loader, minecraft: &str) -> Result<Option<String>> {
        loaders::latest_stable(&self.dl, loader, minecraft).await
    }

    /// Download everything a version needs: client jar, libraries, assets and Java.
    /// Files already on disk are skipped, so this is cheap to call before every launch.
    pub async fn install(&self, game: &GameVersion, game_dir: &Path, progress: &Progress) -> Result<InstalledVersion> {
        progress.stage("Reading version info", 0);
        let version = self.resolve_version(game).await?;
        // The client jar and natives belong to the vanilla version, even when modded.
        let id = game.minecraft.as_str();

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
        game: &GameVersion,
        options: &LaunchOptions,
        progress: &Progress,
    ) -> Result<tokio::process::Command> {
        let game_dir = &options.game_dir;
        tokio::fs::create_dir_all(game_dir).await.at(game_dir)?;

        let installed = self.install(game, game_dir, progress).await?;
        let args = build_arguments(&LaunchContext {
            version: &installed.version,
            account: &options.account,
            env: &self.env,
            game_dir,
            assets_root: &self.paths.assets_dir(),
            game_assets: &installed.game_assets,
            libraries_dir: &self.paths.libraries_dir(),
            natives_dir: &installed.natives_dir,
            classpath: &installed.classpath,
            log_config: installed.log_config.as_deref(),
            memory_mb: options.memory_mb,
        });

        let mut cmd = tokio::process::Command::new(&installed.java);
        cmd.args(args).current_dir(game_dir);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        Ok(cmd)
    }
}
