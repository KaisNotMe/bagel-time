//! Mod loaders. Fabric and Quilt publish ready-made version profiles through
//! their metadata APIs; Forge and NeoForge will need their installers (later).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::download::Downloader;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
}

impl Loader {
    pub fn slug(self) -> &'static str {
        match self {
            Loader::Vanilla => "vanilla",
            Loader::Fabric => "fabric",
            Loader::Quilt => "quilt",
        }
    }

    fn meta_base(self) -> Option<&'static str> {
        match self {
            Loader::Vanilla => None,
            Loader::Fabric => Some("https://meta.fabricmc.net/v2"),
            Loader::Quilt => Some("https://meta.quiltmc.org/v3"),
        }
    }
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Loader::Vanilla => "Vanilla",
            Loader::Fabric => "Fabric",
            Loader::Quilt => "Quilt",
        })
    }
}

impl FromStr for Loader {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "vanilla" => Ok(Loader::Vanilla),
            "fabric" => Ok(Loader::Fabric),
            "quilt" => Ok(Loader::Quilt),
            other => Err(format!("unknown loader '{other}' (expected vanilla, fabric or quilt)")),
        }
    }
}

/// Exactly what to launch: a Minecraft version plus an optional loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    pub minecraft: String,
    pub loader: Loader,
    /// Required for every loader except vanilla.
    pub loader_version: Option<String>,
}

impl GameVersion {
    pub fn vanilla(minecraft: &str) -> Self {
        Self {
            minecraft: minecraft.to_string(),
            loader: Loader::Vanilla,
            loader_version: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}

#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    /// Fabric marks stable builds; Quilt doesn't, so we go by the version name.
    stable: Option<bool>,
}

fn meta_url(loader: Loader, segments: &[&str]) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(loader.meta_base()?).ok()?;
    url.path_segments_mut().ok()?.extend(segments);
    Some(url)
}

/// Loader versions available for a Minecraft version, newest first. Empty if
/// the loader doesn't support that version.
pub async fn loader_versions(dl: &Downloader, loader: Loader, minecraft: &str) -> Result<Vec<LoaderVersion>> {
    let Some(url) = meta_url(loader, &["versions", "loader", minecraft]) else {
        return Ok(Vec::new());
    };
    let entries: Vec<MetaEntry> = match dl.get_json(url.as_str()).await {
        Ok(e) => e,
        // Unknown game versions come back as 400/404 from some meta servers.
        Err(Error::Http(e)) if e.status().is_some_and(|s| s.is_client_error()) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut versions: Vec<LoaderVersion> = entries
        .into_iter()
        .map(|e| LoaderVersion {
            // Fabric only flags its newest build as stable, so a plain release
            // number (no "-beta" etc.) counts as stable too.
            stable: e.loader.stable.unwrap_or(false) || !e.loader.version.contains('-'),
            version: e.loader.version,
        })
        .collect();
    // Quilt's list isn't reliably ordered.
    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    Ok(versions)
}

/// Compare dotted version strings numerically where possible, treating
/// `1.0.0-beta.2` as older than `1.0.0`.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    fn split(v: &str) -> (&str, Option<&str>) {
        let v = v.split('+').next().unwrap_or(v);
        match v.split_once('-') {
            Some((base, pre)) => (base, Some(pre)),
            None => (v, None),
        }
    }
    fn cmp_parts(a: &str, b: &str) -> Ordering {
        let mut a = a.split('.');
        let mut b = b.split('.');
        loop {
            match (a.next(), b.next()) {
                (None, None) => return Ordering::Equal,
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (Some(x), Some(y)) => {
                    let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                        (Ok(x), Ok(y)) => x.cmp(&y),
                        _ => x.cmp(y),
                    };
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
            }
        }
    }

    let (base_a, pre_a) = split(a);
    let (base_b, pre_b) = split(b);
    cmp_parts(base_a, base_b).then_with(|| match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => cmp_parts(x, y),
    })
}

/// Newest stable loader version for a Minecraft version, if any.
pub async fn latest_stable(dl: &Downloader, loader: Loader, minecraft: &str) -> Result<Option<String>> {
    let versions = loader_versions(dl, loader, minecraft).await?;
    Ok(versions
        .iter()
        .find(|v| v.stable)
        .or(versions.first())
        .map(|v| v.version.clone()))
}

pub(crate) fn profile_url(loader: Loader, minecraft: &str, loader_version: &str) -> Option<String> {
    meta_url(
        loader,
        &["versions", "loader", minecraft, loader_version, "profile", "json"],
    )
    .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls_are_escaped() {
        assert_eq!(
            profile_url(Loader::Fabric, "1.21.4", "0.16.10").unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.4/0.16.10/profile/json"
        );
        assert_eq!(
            profile_url(Loader::Quilt, "1.14 Pre-Release 1", "0.27.1").unwrap(),
            "https://meta.quiltmc.org/v3/versions/loader/1.14%20Pre-Release%201/0.27.1/profile/json"
        );
        assert!(profile_url(Loader::Vanilla, "1.21", "x").is_none());
    }

    #[test]
    fn version_ordering() {
        let mut v = vec!["0.20.0-beta.7", "0.19.5", "0.20.0-beta.10", "0.20.0", "0.9.1", "0.20.0-beta.9"];
        v.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(
            v,
            ["0.20.0", "0.20.0-beta.10", "0.20.0-beta.9", "0.20.0-beta.7", "0.19.5", "0.9.1"]
        );
    }

    #[test]
    fn parses_loader_names() {
        assert_eq!("Fabric".parse::<Loader>().unwrap(), Loader::Fabric);
        assert!("forge".parse::<Loader>().is_err());
    }
}
