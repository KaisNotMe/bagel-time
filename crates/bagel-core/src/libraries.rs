use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::download::DownloadJob;
use crate::error::{IoContext, Result};
use crate::meta::{Download, Library, maven_path};
use crate::rules::{Environment, rules_allow};

const LIBRARIES_URL: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Default)]
pub struct ResolvedLibraries {
    /// Jars for the classpath, in version JSON order.
    pub classpath: Vec<PathBuf>,
    /// Old-style native jars whose contents get extracted to the natives folder.
    pub natives: Vec<NativeJar>,
    pub jobs: Vec<DownloadJob>,
}

#[derive(Debug, Clone)]
pub struct NativeJar {
    pub path: PathBuf,
    pub exclude: Vec<String>,
}

/// Work out which library files this machine needs and where they go.
pub fn resolve(libraries: &[Library], libraries_dir: &Path, env: &Environment) -> ResolvedLibraries {
    let mut out = ResolvedLibraries::default();
    let mut on_classpath = HashSet::new();

    for lib in libraries.iter().filter(|l| rules_allow(&l.rules, env)) {
        let downloads = lib.downloads.as_ref();

        if let Some(classifier) = lib.natives.as_ref().and_then(|n| n.get(&env.os_name)) {
            let classifier = classifier.replace("${arch}", env.bits());
            if let Some(dl) = downloads.and_then(|d| d.classifiers.get(&classifier)) {
                let rel = dl
                    .path
                    .clone()
                    .or_else(|| maven_path(&format!("{}:{classifier}", lib.name)));
                if let Some(rel) = rel {
                    let path = libraries_dir.join(rel);
                    out.jobs.push(job(dl, path.clone()));
                    out.natives.push(NativeJar {
                        path,
                        exclude: lib.extract.clone().unwrap_or_default().exclude,
                    });
                }
            }
        }

        let artifact = match downloads.and_then(|d| d.artifact.as_ref()) {
            Some(dl) => dl
                .path
                .clone()
                .or_else(|| maven_path(&lib.name))
                .map(|rel| job(dl, libraries_dir.join(rel))),
            // Mod loader libraries only give a Maven repo, no hashes.
            None if lib.downloads.is_none() && lib.natives.is_none() => maven_path(&lib.name).map(|rel| {
                let base = lib.url.as_deref().unwrap_or(LIBRARIES_URL);
                DownloadJob {
                    url: format!("{}/{rel}", base.trim_end_matches('/')),
                    path: libraries_dir.join(rel),
                    sha1: None,
                    size: None,
                }
            }),
            None => None,
        };
        if let Some(job) = artifact {
            if on_classpath.insert(job.path.clone()) {
                out.classpath.push(job.path.clone());
            }
            out.jobs.push(job);
        }
    }
    out
}

fn job(dl: &Download, path: PathBuf) -> DownloadJob {
    DownloadJob {
        url: dl.url.clone(),
        path,
        sha1: dl.sha1.clone(),
        size: dl.size,
    }
}

/// Unpack old-style native jars (LWJGL 2 era) into `dir`.
pub(crate) async fn extract_natives(natives: Vec<NativeJar>, dir: PathBuf) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&dir).at(&dir)?;
        for jar in &natives {
            let file = std::fs::File::open(&jar.path).at(&jar.path)?;
            let mut zip = zip::ZipArchive::new(file)?;
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i)?;
                let name = entry.name().to_string();
                if entry.is_dir()
                    || name.starts_with("META-INF/")
                    || jar.exclude.iter().any(|e| name.starts_with(e.as_str()))
                {
                    continue;
                }
                let Some(rel) = entry.enclosed_name() else {
                    continue;
                };
                let out = dir.join(rel);
                // Skip files already extracted; they may be locked by a running game.
                if out.metadata().map(|m| m.len() == entry.size()).unwrap_or(false) {
                    continue;
                }
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).at(parent)?;
                }
                let mut f = std::fs::File::create(&out).at(&out)?;
                std::io::copy(&mut entry, &mut f).at(&out)?;
            }
        }
        Ok(())
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env() -> Environment {
        Environment {
            os_name: "windows".into(),
            arch: "x86_64".into(),
            features: HashMap::new(),
        }
    }

    fn libs(json: &str) -> Vec<Library> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn modern_artifact_and_os_filtering() {
        let l = libs(
            r#"[
            {"name":"a:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar","url":"https://x/a/b/1/b-1.jar","sha1":"s","size":1}}},
            {"name":"a:mac:1","downloads":{"artifact":{"path":"a/mac/1/mac-1.jar","url":"https://x/mac","sha1":"s","size":1}},
             "rules":[{"action":"allow","os":{"name":"osx"}}]}
        ]"#,
        );
        let r = resolve(&l, Path::new("L"), &env());
        assert_eq!(r.classpath, vec![Path::new("L").join("a/b/1/b-1.jar")]);
        assert_eq!(r.jobs.len(), 1);
        assert!(r.natives.is_empty());
    }

    #[test]
    fn legacy_natives_use_arch_classifier() {
        let l = libs(
            r#"[{"name":"tv.twitch:twitch-platform:5.16",
              "natives":{"windows":"natives-windows-${arch}"},
              "extract":{"exclude":["META-INF/"]},
              "downloads":{"classifiers":{"natives-windows-64":{"path":"t/n64.jar","url":"https://x/n64","sha1":"s","size":1}}}}]"#,
        );
        let r = resolve(&l, Path::new("L"), &env());
        assert!(r.classpath.is_empty());
        assert_eq!(r.natives.len(), 1);
        assert_eq!(r.natives[0].path, Path::new("L").join("t/n64.jar"));
        assert_eq!(r.natives[0].exclude, vec!["META-INF/"]);
    }

    #[test]
    fn maven_style_library() {
        let l = libs(r#"[{"name":"net.fabricmc:fabric-loader:0.16.0","url":"https://maven.fabricmc.net/"}]"#);
        let r = resolve(&l, Path::new("L"), &env());
        assert_eq!(
            r.jobs[0].url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
        );
        assert_eq!(r.classpath.len(), 1);
    }
}
