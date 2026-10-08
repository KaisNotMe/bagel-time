use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{DownloadJob, Downloader};
use crate::error::{IoContext, Result, parse_json};
use crate::meta::AssetIndexRef;
use crate::paths::Paths;
use crate::progress::Progress;

const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    /// Pre-1.7.3 versions read assets by name from a "virtual" folder.
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    /// Very old versions read them from `<game dir>/resources`.
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// Download the asset index and every object in it. Returns the folder to use
/// for `${game_assets}`, which only differs from the assets root for legacy versions.
pub(crate) async fn install_assets(
    dl: &Downloader,
    paths: &Paths,
    index_ref: &AssetIndexRef,
    game_dir: &Path,
    progress: &Progress,
) -> Result<PathBuf> {
    let index_path = paths.asset_index(&index_ref.id);
    dl.fetch(&DownloadJob {
        url: index_ref.url.clone(),
        path: index_path.clone(),
        sha1: Some(index_ref.sha1.clone()),
        size: Some(index_ref.size),
    })
    .await?;
    let bytes = tokio::fs::read(&index_path).await.at(&index_path)?;
    let index: AssetIndex = parse_json(&bytes, "asset index")?;

    let jobs = index
        .objects
        .values()
        .map(|o| DownloadJob {
            url: format!("{RESOURCES_URL}/{}/{}", &o.hash[..2], o.hash),
            path: paths.asset_object(&o.hash),
            sha1: Some(o.hash.clone()),
            size: Some(o.size),
        })
        .collect();
    dl.fetch_all("Downloading assets", jobs, progress).await?;

    let target = if index.map_to_resources {
        game_dir.join("resources")
    } else if index.is_virtual {
        paths.virtual_assets_dir(&index_ref.id)
    } else {
        return Ok(paths.assets_dir());
    };

    progress.stage("Copying legacy assets", 0);
    let copies: Vec<(PathBuf, PathBuf, u64)> = index
        .objects
        .iter()
        .map(|(name, o)| (paths.asset_object(&o.hash), target.join(name), o.size))
        .collect();
    tokio::task::spawn_blocking(move || -> Result<()> {
        for (from, to, size) in copies {
            if to.metadata().map(|m| m.len() == size).unwrap_or(false) {
                continue;
            }
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            std::fs::copy(&from, &to).at(&to)?;
        }
        Ok(())
    })
    .await??;
    Ok(target)
}
