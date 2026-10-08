//! Java runtimes from Mojang's runtime manifest, so players never need to
//! install Java themselves. Each version JSON names the component it needs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{DownloadJob, Downloader};
use crate::error::{Error, IoContext, Result};
use crate::meta::Download;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::Environment;

const RUNTIMES_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const MARKER: &str = ".bagel-installed";

/// Component used by versions whose JSON predates `javaVersion` (Java 8).
pub const LEGACY_COMPONENT: &str = "jre-legacy";

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: Download,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        downloads: RuntimeDownloads,
        #[serde(default)]
        executable: bool,
    },
    Directory,
    Link {
        #[cfg_attr(not(unix), allow(dead_code))]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct RuntimeDownloads {
    raw: Download,
}

fn platform_key(env: &Environment) -> Option<&'static str> {
    Some(match (env.os_name.as_str(), env.arch.as_str()) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "x86") => "windows-x86",
        ("windows", "arm64") => "windows-arm64",
        ("osx", "arm64") => "mac-os-arm64",
        ("osx", _) => "mac-os",
        ("linux", "x86") => "linux-i386",
        ("linux", _) => "linux",
        _ => return None,
    })
}

pub fn java_executable(runtime_dir: &Path, env: &Environment) -> PathBuf {
    match env.os_name.as_str() {
        "windows" => runtime_dir.join("bin").join("java.exe"),
        "osx" => runtime_dir.join("jre.bundle/Contents/Home/bin/java"),
        _ => runtime_dir.join("bin").join("java"),
    }
}

/// Make sure the runtime `component` is installed and return its java executable.
/// Works offline once installed.
pub(crate) async fn ensure_runtime(
    dl: &Downloader,
    paths: &Paths,
    env: &Environment,
    component: &str,
    progress: &Progress,
) -> Result<PathBuf> {
    let dir = paths.java_dir(component);
    let exe = java_executable(&dir, env);
    if dir.join(MARKER).exists() && exe.exists() {
        return Ok(exe);
    }

    let not_available = || Error::NoJavaRuntime {
        component: component.to_string(),
        platform: format!("{}-{}", env.os_name, env.arch),
    };
    let platform = platform_key(env).ok_or_else(not_available)?;
    progress.stage("Looking up Java", 0);
    let all: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> = dl.get_json(RUNTIMES_URL).await?;
    let entry = all
        .get(platform)
        .and_then(|p| p.get(component))
        .and_then(|v| v.first())
        .ok_or_else(not_available)?;
    let manifest: RuntimeManifest = dl.get_json(&entry.manifest.url).await?;

    let mut jobs = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (rel, file) in &manifest.files {
        let path = dir.join(rel);
        match file {
            RuntimeFile::Directory => tokio::fs::create_dir_all(&path).await.at(&path)?,
            RuntimeFile::File { downloads, executable } => {
                if *executable {
                    executables.push(path.clone());
                }
                jobs.push(DownloadJob {
                    url: downloads.raw.url.clone(),
                    path,
                    sha1: downloads.raw.sha1.clone(),
                    size: downloads.raw.size,
                });
            }
            RuntimeFile::Link { target } => links.push((path, target.clone())),
        }
    }
    dl.fetch_all(&format!("Downloading Java {}", entry.version.name), jobs, progress)
        .await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in &executables {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).at(path)?;
        }
        for (path, target) in &links {
            if !path.exists() {
                std::os::unix::fs::symlink(target, path).at(path)?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = (executables, links);

    let marker = dir.join(MARKER);
    tokio::fs::write(&marker, &entry.version.name).await.at(&marker)?;
    Ok(exe)
}
