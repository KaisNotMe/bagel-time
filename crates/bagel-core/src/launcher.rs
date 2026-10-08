use std::path::{Path, PathBuf};

use crate::account::Account;
use crate::assets::install_assets;
use crate::download::{DownloadJob, Downloader};
use crate::forge;
use crate::error::{Error, IoContext, Result, parse_json};
use crate::java::{LEGACY_COMPONENT, ensure_runtime};
use crate::launch::{LaunchContext, build_arguments, supports_quick_play};
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
    /// Extra JVM arguments, e.g. from instance settings.
    pub extra_jvm_args: Vec<String>,
    /// Join this server (`host` or `host:port`) once the game starts.
    pub server: Option<String>,
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
    async fn resolve_version(&self, game: &GameVersion, progress: &Progress) -> Result<VersionJson> {
        let vanilla = self.version_json(&game.minecraft).await?;
        match (game.loader, game.loader_version.as_deref()) {
            (Loader::Vanilla, _) => Ok(vanilla),
            (loader, Some(loader_version)) if loader.uses_installer() => {
                let profile =
                    forge::load_profile(&self.dl, &self.paths, loader, &game.minecraft, loader_version, progress).await?;
                Ok(vanilla.with_profile(profile))
            }
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
        let version = self.resolve_version(game, progress).await?;
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

        let jar = if game.loader.uses_installer() {
            let ctx = forge::ProcessorContext {
                dl: &self.dl,
                paths: &self.paths,
                env: &self.env,
                loader: game.loader,
                minecraft: &game.minecraft,
                loader_version: game.loader_version.as_deref().unwrap_or_default(),
                client_jar: &jar,
                java: &java,
            };
            forge::run_processors(&ctx, progress).await?;
            // Forge tells its launcher to skip `<version id>.jar` on the classpath,
            // as Mojang's launcher names the inherited jar that way.
            let named = self.paths.version_jar(&version.id);
            if named.metadata().map(|m| m.len()).ok() != jar.metadata().map(|m| m.len()).ok() {
                if let Some(parent) = named.parent() {
                    tokio::fs::create_dir_all(parent).await.at(parent)?;
                }
                tokio::fs::copy(&jar, &named).await.at(&named)?;
            }
            named
        } else {
            jar
        };

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
        // Old versions don't look up SRV records for --server, so do it here.
        let server = match &options.server {
            Some(s) if !supports_quick_play(&installed.version) => Some(match crate::servers::resolve(s).await {
                Ok((host, port)) if host.contains(':') => format!("[{host}]:{port}"),
                Ok((host, port)) => format!("{host}:{port}"),
                Err(_) => s.clone(),
            }),
            other => other.clone(),
        };
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
            extra_jvm_args: &options.extra_jvm_args,
            server: server.as_deref(),
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

    /// Downloads (once) what a hosted server needs to run in `dir`, then
    /// returns a ready-to-spawn command. Java and the server files are kept
    /// between starts; changing the version reinstalls.
    pub async fn prepare_server(
        &self,
        game: &GameVersion,
        dir: &Path,
        memory_mb: u32,
        progress: &Progress,
    ) -> Result<tokio::process::Command> {
        tokio::fs::create_dir_all(dir).await.at(dir)?;
        // Installers run inside the server folder, so relative paths would break.
        let dir = &std::path::absolute(dir).at(dir)?;
        let vanilla = self.version_json(&game.minecraft).await?;
        let component = vanilla
            .java_version
            .as_ref()
            .map_or(LEGACY_COMPONENT, |j| j.component.as_str());
        let java = ensure_runtime(&self.dl, &self.paths, &self.env, component, progress).await?;

        let marker = dir.join(SERVER_MARKER);
        let wanted = format!(
            "{} {} {}",
            game.minecraft,
            game.loader.slug(),
            game.loader_version.as_deref().unwrap_or("")
        );
        let previous: Option<ServerInstall> = tokio::fs::read(&marker)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let args = match previous {
            Some(p) if p.game == wanted => p.args,
            _ => {
                let args = self.install_server(game, &vanilla, &java, dir, progress).await?;
                let record = ServerInstall { game: wanted, args: args.clone() };
                let json = serde_json::to_vec_pretty(&record).expect("record serializes");
                tokio::fs::write(&marker, json).await.at(&marker)?;
                args
            }
        };

        let mut cmd = tokio::process::Command::new(&java);
        cmd.arg(format!("-Xmx{memory_mb}M")).args(&args).current_dir(dir);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        Ok(cmd)
    }

    /// Puts the server files in place; returns the arguments after `-Xmx`.
    async fn install_server(
        &self,
        game: &GameVersion,
        vanilla: &VersionJson,
        java: &Path,
        dir: &Path,
        progress: &Progress,
    ) -> Result<Vec<String>> {
        let args = |jar: &str| vec!["-jar".to_string(), jar.to_string(), "nogui".to_string()];
        let loader_version = || {
            game.loader_version
                .clone()
                .ok_or_else(|| Error::MissingLoaderVersion(game.loader.to_string()))
        };
        let vanilla_jar = || async {
            let download = vanilla.downloads.get("server").ok_or_else(|| {
                Error::Invalid(format!("Mojang doesn't publish a server for Minecraft {}.", game.minecraft))
            })?;
            progress.stage("Downloading the server", 1);
            self.dl
                .fetch(&DownloadJob {
                    url: download.url.clone(),
                    path: dir.join("server.jar"),
                    sha1: download.sha1.clone(),
                    size: download.size,
                })
                .await?;
            progress.advance(1);
            Ok::<_, Error>(())
        };

        match game.loader {
            Loader::Vanilla => {
                vanilla_jar().await?;
                Ok(args("server.jar"))
            }
            Loader::Fabric => {
                vanilla_jar().await?;
                #[derive(serde::Deserialize)]
                struct Installer {
                    version: String,
                    #[serde(default)]
                    stable: bool,
                }
                let installers: Vec<Installer> =
                    self.dl.get_json("https://meta.fabricmc.net/v2/versions/installer").await?;
                let installer = installers
                    .iter()
                    .find(|i| i.stable)
                    .or(installers.first())
                    .ok_or_else(|| Error::Installer("Fabric has no server installer right now.".into()))?;
                let url = format!(
                    "https://meta.fabricmc.net/v2/versions/loader/{}/{}/{}/server/jar",
                    game.minecraft,
                    loader_version()?,
                    installer.version
                );
                let jar = "fabric-server-launch.jar";
                self.dl
                    .fetch(&DownloadJob { url, path: dir.join(jar), sha1: None, size: None })
                    .await?;
                Ok(args(jar))
            }
            Loader::Quilt => {
                vanilla_jar().await?;
                #[derive(serde::Deserialize)]
                struct Installer {
                    url: String,
                    version: String,
                }
                let installers: Vec<Installer> =
                    self.dl.get_json("https://meta.quiltmc.org/v3/versions/installer").await?;
                let installer = installers
                    .first()
                    .ok_or_else(|| Error::Installer("Quilt has no server installer right now.".into()))?;
                let path = self.paths.cache_dir().join(format!("quilt-installer-{}.jar", installer.version));
        let path = std::path::absolute(&path).at(&path)?;
                self.dl
                    .fetch(&DownloadJob { url: installer.url.clone(), path: path.clone(), sha1: None, size: None })
                    .await?;
                progress.stage("Installing Quilt", 1);
                run_installer(
                    java,
                    dir,
                    &[
                        "-jar".into(),
                        path.display().to_string(),
                        "install".into(),
                        "server".into(),
                        game.minecraft.clone(),
                        loader_version()?,
                        format!("--install-dir={}", dir.display()),
                    ],
                    "Quilt",
                )
                .await?;
                progress.advance(1);
                Ok(args("quilt-server-launch.jar"))
            }
            Loader::Forge | Loader::NeoForge => {
                let artifact = forge::artifact(game.loader, &game.minecraft, &loader_version()?);
                let url = artifact.installer_url();
                let file = url.rsplit('/').next().unwrap_or("installer.jar").to_string();
                let path = self.paths.cache_dir().join("installers").join(file);
                let path = std::path::absolute(&path).at(&path)?;
                self.dl
                    .fetch(&DownloadJob { url, path: path.clone(), sha1: None, size: None })
                    .await?;
                progress.stage(&format!("Installing {} (this can take a few minutes)", game.loader), 1);
                run_installer(
                    java,
                    dir,
                    &["-jar".into(), path.display().to_string(), "--installServer".into(), dir.display().to_string()],
                    &game.loader.to_string(),
                )
                .await?;
                progress.advance(1);
                forge_server_args(dir, artifact.path, &self.env)
                    .await?
                    .ok_or_else(|| Error::Installer(format!("The {} installer didn't leave a server to run.", game.loader)))
            }
        }
    }
}

const SERVER_MARKER: &str = ".bagel-server.json";

/// Remembered after installing a server, so later starts skip it.
#[derive(serde::Serialize, serde::Deserialize)]
struct ServerInstall {
    /// "<minecraft> <loader> <loader version>".
    game: String,
    args: Vec<String>,
}

async fn run_installer(java: &Path, dir: &Path, args: &[String], what: &str) -> Result<()> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| Error::Installer(format!("Couldn't start Java for the {what} installer: {e}")))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr).into_owned() + &String::from_utf8_lossy(&output.stdout);
        let tail: Vec<&str> = text.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(Error::Installer(format!("The {what} server installer failed:\n{}", tail.join("\n"))));
    }
    Ok(())
}

/// How to start a Forge or NeoForge server the installer just set up: modern
/// ones ship an argument file (`@libraries/.../win_args.txt`), old ones a jar.
async fn forge_server_args(dir: &Path, maven_path: &str, env: &Environment) -> Result<Option<Vec<String>>> {
    let file = if env.os_name == "windows" { "win_args.txt" } else { "unix_args.txt" };
    let base = dir.join("libraries").join(maven_path);
    if let Ok(mut versions) = tokio::fs::read_dir(&base).await {
        while let Some(entry) = versions.next_entry().await.at(&base)? {
            if entry.path().join(file).is_file() {
                let rel = format!("libraries/{maven_path}/{}/{file}", entry.file_name().to_string_lossy());
                return Ok(Some(vec![format!("@{rel}"), "nogui".into()]));
            }
        }
    }
    let mut entries = tokio::fs::read_dir(dir).await.at(dir)?;
    while let Some(entry) = entries.next_entry().await.at(dir)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".jar")
            && !name.contains("installer")
            && (name.starts_with("forge-") || name.starts_with("neoforge-") || name.contains("universal"))
        {
            return Ok(Some(vec!["-jar".into(), name, "nogui".into()]));
        }
    }
    Ok(None)
}
