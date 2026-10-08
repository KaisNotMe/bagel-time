//! Forge and NeoForge.
//!
//! Neither publishes ready-made launcher profiles. Instead, their installer
//! jar carries:
//! - `install_profile.json`: libraries for its tools, a `data` table and a list
//!   of "processors" (small Java programs) that patch the Minecraft jar;
//! - `version.json`: the launcher profile, layered on vanilla like Fabric's;
//! - `maven/`: libraries that aren't downloadable anywhere else.
//!
//! Old Forge (1.12.2 and earlier) uses a simpler format: `install_profile.json`
//! holds an `install` section and the profile itself (`versionInfo`), and the
//! Forge jar sits in the installer root.
//!
//! Installing happens in two steps. [`load_profile`] downloads the installer,
//! unpacks the profile and the embedded libraries. [`run_processors`] runs the
//! patching tools once the vanilla jar, libraries and Java are in place. Both
//! remember what they did, so later launches skip them.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;

use crate::download::{DownloadJob, Downloader, sha1_file};
use crate::error::{Error, IoContext, Result, parse_json};
use crate::libraries::resolve;
use crate::loaders::{Loader, LoaderVersion, compare_versions};
use crate::meta::{Library, LoaderProfile, maven_path};
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::Environment;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
/// Written next to the cached profile once the processors have run.
const DONE_MARKER: &str = "installed";

/// The Maven artifact and full version for a loader version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Artifact {
    pub repo: &'static str,
    /// e.g. `net/minecraftforge/forge`.
    pub path: &'static str,
    /// e.g. `1.20.1-47.3.0` or `21.1.77`.
    pub version: String,
}

impl Artifact {
    pub fn installer_url(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(self.path);
        format!("{}/{}/{}/{name}-{}-installer.jar", self.repo, self.path, self.version, self.version)
    }

    fn metadata_url(repo: &str, path: &str) -> String {
        format!("{repo}/{path}/maven-metadata.xml")
    }
}

/// Where a Forge or NeoForge version lives. Forge versions are stored without
/// the Minecraft prefix (`47.3.0`), NeoForge ones as published (`21.1.77`).
pub(crate) fn artifact(loader: Loader, minecraft: &str, loader_version: &str) -> Artifact {
    match loader {
        // NeoForge for 1.20.1 was published under the old Forge coordinates.
        Loader::NeoForge if minecraft == "1.20.1" => Artifact {
            repo: NEOFORGE_MAVEN,
            path: "net/neoforged/forge",
            version: format!("{minecraft}-{loader_version}"),
        },
        Loader::NeoForge => Artifact {
            repo: NEOFORGE_MAVEN,
            path: "net/neoforged/neoforge",
            version: loader_version.to_string(),
        },
        _ => Artifact {
            repo: FORGE_MAVEN,
            path: "net/minecraftforge/forge",
            version: format!("{minecraft}-{loader_version}"),
        },
    }
}

/// All `<version>` entries in a Maven metadata file.
fn metadata_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split_once("</version>").map(|(v, _)| v.trim().to_string()))
        .collect()
}

/// The prefix NeoForge versions for a Minecraft version start with:
/// 1.21.1 -> `21.1.`, 1.21 -> `21.0.`, 26.1 -> `26.1.0.`, 26.1.2 -> `26.1.2.`.
fn neoforge_prefix(minecraft: &str) -> Option<String> {
    let parts: Vec<&str> = minecraft.split('.').collect();
    if !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    match parts.as_slice() {
        ["1", minor] => Some(format!("{minor}.0.")),
        ["1", minor, patch] => Some(format!("{minor}.{patch}.")),
        [year, drop] => Some(format!("{year}.{drop}.0.")),
        [year, drop, patch] => Some(format!("{year}.{drop}.{patch}.")),
        _ => None,
    }
}

/// Loader versions for a Minecraft version, newest first.
pub async fn versions(dl: &Downloader, loader: Loader, minecraft: &str) -> Result<Vec<LoaderVersion>> {
    let mut out: Vec<LoaderVersion> = match loader {
        Loader::Forge => {
            let xml = dl.get_bytes(&Artifact::metadata_url(FORGE_MAVEN, "net/minecraftforge/forge")).await?;
            let prefix = format!("{minecraft}-");
            metadata_versions(&String::from_utf8_lossy(&xml))
                .into_iter()
                .filter_map(|v| v.strip_prefix(&prefix).map(String::from))
                .map(|version| LoaderVersion { version, stable: true })
                .collect()
        }
        Loader::NeoForge if minecraft == "1.20.1" => {
            let xml = dl.get_bytes(&Artifact::metadata_url(NEOFORGE_MAVEN, "net/neoforged/forge")).await?;
            metadata_versions(&String::from_utf8_lossy(&xml))
                .into_iter()
                .filter_map(|v| v.strip_prefix("1.20.1-").map(String::from))
                .map(|version| LoaderVersion { version, stable: true })
                .collect()
        }
        Loader::NeoForge => {
            let Some(prefix) = neoforge_prefix(minecraft) else {
                return Ok(Vec::new());
            };
            let xml = dl.get_bytes(&Artifact::metadata_url(NEOFORGE_MAVEN, "net/neoforged/neoforge")).await?;
            metadata_versions(&String::from_utf8_lossy(&xml))
                .into_iter()
                // Alphas track snapshots of the next Minecraft version.
                .filter(|v| v.starts_with(&prefix) && !v.contains("alpha") && !v.contains('+'))
                .map(|version| LoaderVersion {
                    stable: !version.contains("beta"),
                    version,
                })
                .collect()
        }
        _ => Vec::new(),
    };
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallProfile {
    /// Modern format.
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    data: HashMap<String, SidedValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
    /// Old format.
    #[serde(default)]
    install: Option<LegacyInstall>,
    #[serde(default)]
    version_info: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SidedValue {
    client: String,
}

#[derive(Debug, Deserialize)]
struct Processor {
    #[serde(default)]
    sides: Option<Vec<String>>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyInstall {
    /// Maven coordinate the Forge jar should be stored under.
    path: String,
    /// The Forge jar's name inside the installer.
    file_path: String,
}

struct Locations {
    installer: PathBuf,
    profile: PathBuf,
    install_profile: PathBuf,
    data_dir: PathBuf,
    marker: PathBuf,
}

fn locations(paths: &Paths, loader: Loader, minecraft: &str, loader_version: &str) -> Locations {
    let profile = paths.loader_profile(loader.slug(), minecraft, loader_version);
    let dir = profile.parent().map(Path::to_path_buf).unwrap_or_default();
    Locations {
        installer: dir.join("installer.jar"),
        install_profile: dir.join("install_profile.json"),
        data_dir: dir.join("data"),
        marker: dir.join(DONE_MARKER),
        profile,
    }
}

/// Downloads the installer (once), unpacks the launcher profile and the
/// libraries it embeds, and returns the profile.
pub(crate) async fn load_profile(
    dl: &Downloader,
    paths: &Paths,
    loader: Loader,
    minecraft: &str,
    loader_version: &str,
    progress: &Progress,
) -> Result<LoaderProfile> {
    let loc = locations(paths, loader, minecraft, loader_version);
    if !loc.profile.is_file() || !loc.install_profile.is_file() {
        progress.stage(&format!("Downloading the {loader} installer"), 0);
        fetch_installer(dl, loader, minecraft, loader_version, &loc.installer).await?;
        let (installer, profile, install_profile, libraries) =
            (loc.installer.clone(), loc.profile.clone(), loc.install_profile.clone(), paths.libraries_dir());
        tokio::task::spawn_blocking(move || unpack_installer(&installer, &profile, &install_profile, &libraries))
            .await??;
    }
    let bytes = tokio::fs::read(&loc.profile).await.at(&loc.profile)?;
    parse_json(&bytes, &format!("{loader} {loader_version} profile"))
}

async fn fetch_installer(
    dl: &Downloader,
    loader: Loader,
    minecraft: &str,
    loader_version: &str,
    dest: &Path,
) -> Result<()> {
    let mut art = artifact(loader, minecraft, loader_version);
    let mut job = DownloadJob { url: art.installer_url(), path: dest.to_path_buf(), sha1: None, size: None };
    // Very old Forge versions carry the Minecraft version twice, e.g.
    // 1.7.10-10.13.4.1614-1.7.10; modpacks often leave the suffix off.
    let first = dl.fetch(&job).await;
    if let Err(Error::Http(e)) = &first
        && loader == Loader::Forge
        && e.status().is_some_and(|s| s.as_u16() == 404)
    {
        art.version = format!("{minecraft}-{loader_version}-{minecraft}");
        job.url = art.installer_url();
        return dl.fetch(&job).await.map_err(|_| not_found(loader, minecraft, loader_version));
    }
    first.map_err(|e| match e {
        Error::Http(h) if h.status().is_some_and(|s| s.as_u16() == 404) => not_found(loader, minecraft, loader_version),
        other => other,
    })
}

fn not_found(loader: Loader, minecraft: &str, loader_version: &str) -> Error {
    Error::Installer(format!("{loader} {loader_version} for Minecraft {minecraft} doesn't exist."))
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut entry = zip.by_name(name.trim_start_matches('/')).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Writes the launcher profile and install profile next to each other and
/// copies embedded libraries into the libraries folder.
fn unpack_installer(installer: &Path, profile: &Path, install_profile: &Path, libraries: &Path) -> Result<()> {
    let file = std::fs::File::open(installer).at(installer)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let raw = read_entry(&mut zip, "install_profile.json")
        .ok_or_else(|| Error::Installer("The installer has no install_profile.json.".into()))?;
    let parsed: InstallProfile = parse_json(&raw, "install_profile.json")?;

    let version_json = if let (Some(install), Some(info)) = (&parsed.install, &parsed.version_info) {
        // Old format: the Forge jar sits in the installer root.
        let rel = maven_path(&install.path)
            .ok_or_else(|| Error::Installer(format!("Bad library name in installer: {}", install.path)))?;
        let mut jar = zip
            .by_name(&install.file_path)
            .map_err(|_| Error::Installer(format!("The installer is missing {}.", install.file_path)))?;
        let dest = libraries.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        let mut out = std::fs::File::create(&dest).at(&dest)?;
        std::io::copy(&mut jar, &mut out).at(&dest)?;
        serde_json::to_vec(info).expect("JSON value serializes")
    } else {
        let name = parsed.json.as_deref().unwrap_or("/version.json");
        read_entry(&mut zip, name).ok_or_else(|| Error::Installer(format!("The installer has no {name}.")))?
    };

    // Libraries that ship inside the installer.
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.name().strip_prefix("maven/").map(String::from) else {
            continue;
        };
        let Some(rel) = crate::mrpack::safe_relative_path(&rel) else {
            continue;
        };
        let dest = libraries.join(rel);
        if dest.metadata().is_ok_and(|m| m.len() == entry.size()) {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        let mut out = std::fs::File::create(&dest).at(&dest)?;
        std::io::copy(&mut entry, &mut out).at(&dest)?;
    }

    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    std::fs::write(install_profile, &raw).at(install_profile)?;
    std::fs::write(profile, version_json).at(profile)
}

/// What the processors need from the rest of the install.
pub(crate) struct ProcessorContext<'a> {
    pub dl: &'a Downloader,
    pub paths: &'a Paths,
    pub env: &'a Environment,
    pub loader: Loader,
    pub minecraft: &'a str,
    pub loader_version: &'a str,
    /// The vanilla client jar.
    pub client_jar: &'a Path,
    pub java: &'a Path,
}

/// Downloads the installer's tools and runs the client-side processors, unless
/// that already happened for this version.
pub(crate) async fn run_processors(ctx: &ProcessorContext<'_>, progress: &Progress) -> Result<()> {
    let loc = locations(ctx.paths, ctx.loader, ctx.minecraft, ctx.loader_version);
    if loc.marker.is_file() {
        return Ok(());
    }
    let raw = tokio::fs::read(&loc.install_profile).await.at(&loc.install_profile)?;
    let profile: InstallProfile = parse_json(&raw, "install_profile.json")?;
    let libraries = ctx.paths.libraries_dir();

    let tools = resolve(&profile.libraries, &libraries, ctx.env);
    ctx.dl
        .fetch_all(&format!("Downloading {} tools", ctx.loader), tools.jobs, progress)
        .await?;

    let processors: Vec<&Processor> = profile
        .processors
        .iter()
        .filter(|p| p.sides.as_ref().is_none_or(|s| s.iter().any(|s| s == "client")))
        .collect();
    if !processors.is_empty() {
        let data = data_table(ctx, &profile, &loc).await?;
        for (i, processor) in processors.iter().enumerate() {
            progress.stage(
                &format!("Installing {} ({} of {})", ctx.loader, i + 1, processors.len()),
                0,
            );
            run_processor(ctx, processor, &data, &libraries).await?;
        }
    }
    tokio::fs::write(&loc.marker, b"").await.at(&loc.marker)
}

/// Values for `{NAME}` placeholders in processor arguments.
async fn data_table(
    ctx: &ProcessorContext<'_>,
    profile: &InstallProfile,
    loc: &Locations,
) -> Result<HashMap<String, String>> {
    let libraries = ctx.paths.libraries_dir();
    let mut data = HashMap::new();
    let mut extract = Vec::new();
    for (key, value) in &profile.data {
        let v = value.client.as_str();
        let resolved = if let Some(coord) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            library_path(&libraries, coord)?
        } else if let Some(lit) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            lit.to_string()
        } else if v.starts_with('/') {
            // A file inside the installer, e.g. /data/client.lzma.
            let rel = crate::mrpack::safe_relative_path(v.trim_start_matches('/'))
                .ok_or_else(|| Error::Installer(format!("Bad data path in installer: {v}")))?;
            let dest = loc.data_dir.join(rel);
            extract.push((v.to_string(), dest.clone()));
            dest.display().to_string()
        } else {
            v.to_string()
        };
        data.insert(key.clone(), resolved);
    }

    let installer = loc.installer.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::open(&installer).at(&installer)?;
        let mut zip = zip::ZipArchive::new(file)?;
        for (name, dest) in extract {
            let bytes = read_entry(&mut zip, &name)
                .ok_or_else(|| Error::Installer(format!("The installer is missing {name}.")))?;
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            std::fs::write(&dest, bytes).at(&dest)?;
        }
        Ok(())
    })
    .await??;

    let builtins = [
        ("SIDE", "client".to_string()),
        ("MINECRAFT_JAR", ctx.client_jar.display().to_string()),
        ("MINECRAFT_VERSION", ctx.minecraft.to_string()),
        ("ROOT", ctx.paths.root().display().to_string()),
        ("INSTALLER", loc.installer.display().to_string()),
        ("LIBRARY_DIR", libraries.display().to_string()),
    ];
    for (k, v) in builtins {
        data.insert(k.to_string(), v);
    }
    Ok(data)
}

fn library_path(libraries: &Path, coord: &str) -> Result<String> {
    maven_path(coord)
        .map(|rel| libraries.join(rel).display().to_string())
        .ok_or_else(|| Error::Installer(format!("Bad library name in installer: {coord}")))
}

/// Fills in `{NAME}` from the data table and turns `[maven:coords]` into a path.
fn substitute(arg: &str, data: &HashMap<String, String>, libraries: &Path) -> Result<String> {
    if let Some(coord) = arg.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return library_path(libraries, coord);
    }
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else { break };
        let key = &rest[start + 1..start + len];
        out.push_str(&rest[..start]);
        match data.get(key) {
            Some(v) => out.push_str(v),
            None => out.push_str(&rest[start..=start + len]),
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The `Main-Class` from a jar's manifest.
fn main_class(jar: &Path) -> Result<String> {
    let file = std::fs::File::open(jar).at(jar)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let manifest = read_entry(&mut zip, "META-INF/MANIFEST.MF")
        .ok_or_else(|| Error::Installer(format!("{} has no manifest.", jar.display())))?;
    String::from_utf8_lossy(&manifest)
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:").map(|c| c.trim().to_string()))
        .ok_or_else(|| Error::Installer(format!("{} has no main class.", jar.display())))
}

/// True when every declared output exists with the expected SHA-1.
async fn outputs_ok(outputs: &[(String, String)]) -> bool {
    for (path, sha1) in outputs {
        match sha1_file(Path::new(path)).await {
            Ok(actual) if actual.eq_ignore_ascii_case(sha1) => {}
            _ => return false,
        }
    }
    true
}

async fn run_processor(
    ctx: &ProcessorContext<'_>,
    p: &Processor,
    data: &HashMap<String, String>,
    libraries: &Path,
) -> Result<()> {
    let outputs: Vec<(String, String)> = p
        .outputs
        .iter()
        .map(|(k, v)| Ok((substitute(k, data, libraries)?, substitute(v, data, libraries)?)))
        .collect::<Result<_>>()?;
    if !outputs.is_empty() && outputs_ok(&outputs).await {
        return Ok(());
    }

    let jar = PathBuf::from(library_path(libraries, &p.jar)?);
    let jar_for_manifest = jar.clone();
    let main = tokio::task::spawn_blocking(move || main_class(&jar_for_manifest)).await??;
    let mut classpath = vec![jar.display().to_string()];
    for coord in &p.classpath {
        classpath.push(library_path(libraries, coord)?);
    }
    let separator = if ctx.env.os_name == "windows" { ";" } else { ":" };
    let args: Vec<String> = p
        .args
        .iter()
        .map(|a| substitute(a, data, libraries))
        .collect::<Result<_>>()?;

    let mut cmd = tokio::process::Command::new(ctx.java);
    cmd.arg("-cp")
        .arg(classpath.join(separator))
        .arg(&main)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| Error::Installer(format!("Couldn't start Java for the {} installer: {e}", ctx.loader)))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr).into_owned() + &String::from_utf8_lossy(&output.stdout);
        let tail: Vec<&str> = text.lines().rev().take(8).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(Error::Installer(format!(
            "The {} installer step {main} failed:\n{}",
            ctx.loader,
            tail.join("\n")
        )));
    }
    if !outputs.is_empty() && !outputs_ok(&outputs).await {
        return Err(Error::Installer(format!(
            "The {} installer step {main} produced unexpected files.",
            ctx.loader
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_and_urls() {
        let f = artifact(Loader::Forge, "1.20.1", "47.3.0");
        assert_eq!(
            f.installer_url(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-installer.jar"
        );
        let n = artifact(Loader::NeoForge, "1.21.1", "21.1.77");
        assert_eq!(
            n.installer_url(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.77/neoforge-21.1.77-installer.jar"
        );
        let old = artifact(Loader::NeoForge, "1.20.1", "47.1.106");
        assert_eq!(
            old.installer_url(),
            "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
    }

    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.21").as_deref(), Some("21.0."));
        assert_eq!(neoforge_prefix("26.1").as_deref(), Some("26.1.0."));
        assert_eq!(neoforge_prefix("26.1.2").as_deref(), Some("26.1.2."));
        assert_eq!(neoforge_prefix("25w14craftmine"), None);
    }

    #[test]
    fn reads_maven_metadata() {
        let xml = "<metadata><versioning><versions><version>21.1.1</version>\n<version>21.1.77</version></versions></versioning></metadata>";
        assert_eq!(metadata_versions(xml), ["21.1.1", "21.1.77"]);
    }

    #[test]
    fn substitutes_processor_arguments() {
        let data = HashMap::from([
            ("ROOT".to_string(), "R".to_string()),
            ("SIDE".to_string(), "client".to_string()),
        ]);
        let libs = Path::new("L");
        assert_eq!(substitute("{ROOT}/run.sh", &data, libs).unwrap(), "R/run.sh");
        assert_eq!(substitute("--{SIDE}-{NOPE}", &data, libs).unwrap(), "--client-{NOPE}");
        assert_eq!(
            substitute("[net.minecraft:client:1.20.1:slim]", &data, libs).unwrap(),
            Path::new("L").join("net/minecraft/client/1.20.1/client-1.20.1-slim.jar").display().to_string()
        );
    }

    #[test]
    fn parses_both_install_profile_formats() {
        let modern: InstallProfile = serde_json::from_str(
            r#"{"spec":1,"json":"/version.json","data":{"PATCHED":{"client":"[a:b:1:client]","server":"x"}},
                "processors":[{"sides":["server"],"jar":"a:b:1","args":[]},{"jar":"c:d:2","args":["--x"],"outputs":{"{A}":"{B}"}}],
                "libraries":[]}"#,
        )
        .unwrap();
        assert_eq!(modern.processors.len(), 2);
        assert_eq!(modern.data["PATCHED"].client, "[a:b:1:client]");

        let legacy: InstallProfile = serde_json::from_str(
            r#"{"install":{"path":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10","filePath":"forge-universal.jar"},
                "versionInfo":{"id":"x","mainClass":"net.minecraft.launchwrapper.Launch","inheritsFrom":"1.7.10","libraries":[]}}"#,
        )
        .unwrap();
        assert!(legacy.install.is_some() && legacy.version_info.is_some());
    }
}
