use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::{StreamExt, TryStreamExt};
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, IoContext, Result, parse_json};
use crate::progress::Progress;

/// Modrinth asks for a User-Agent that identifies the project.
pub const USER_AGENT: &str = concat!(
    "KaisNotMe/bagel-time/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/KaisNotMe/bagel-time)"
);

const CONCURRENCY: usize = 16;
const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client configuration is valid");
        Self { client }
    }

    /// The shared HTTP client, for requests that need extra headers.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self.client.get(url).send().await?.error_for_status()?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        parse_json(&self.get_bytes(url).await?, url)
    }

    pub async fn post_json<B: serde::Serialize + ?Sized, T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self.client.post(url).json(body).send().await?.error_for_status()?;
        parse_json(&resp.bytes().await?, url)
    }

    /// Download one file unless it is already present. Writes to a `.part` file
    /// and renames after the checksum passes, so a half-finished download is
    /// never mistaken for a complete one.
    pub async fn fetch(&self, job: &DownloadJob) -> Result<()> {
        if is_present(job).await? {
            return Ok(());
        }
        if let Some(parent) = job.path.parent() {
            tokio::fs::create_dir_all(parent).await.at(parent)?;
        }
        let mut last_err = None;
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
            }
            match self.try_fetch(job).await {
                Ok(()) => return Ok(()),
                // A disk problem won't fix itself by retrying.
                Err(e @ Error::Io { .. }) => return Err(e),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.expect("at least one attempt ran"))
    }

    async fn try_fetch(&self, job: &DownloadJob) -> Result<()> {
        let tmp = part_path(&job.path);
        let mut resp = self.client.get(&job.url).send().await?.error_for_status()?;
        let mut file = tokio::fs::File::create(&tmp).await.at(&tmp)?;
        let mut hasher = Sha1::new();
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await.at(&tmp)?;
        }
        file.flush().await.at(&tmp)?;
        drop(file);

        let actual = hex::encode(hasher.finalize());
        if let Some(expected) = &job.sha1 {
            if !expected.eq_ignore_ascii_case(&actual) {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(Error::Checksum {
                    url: job.url.clone(),
                    expected: expected.clone(),
                    actual,
                });
            }
        }
        tokio::fs::rename(&tmp, &job.path).await.at(&job.path)
    }

    /// Download many files in parallel, reporting one progress step per file.
    pub async fn fetch_all(&self, stage: &str, mut jobs: Vec<DownloadJob>, progress: &Progress) -> Result<()> {
        let mut seen = HashSet::new();
        jobs.retain(|j| seen.insert(j.path.clone()));
        progress.stage(stage, jobs.len() as u64);
        futures::stream::iter(jobs)
            .map(|job| async move {
                self.fetch(&job).await?;
                progress.advance(1);
                Ok::<_, Error>(())
            })
            .buffer_unordered(CONCURRENCY)
            .try_collect::<()>()
            .await
    }
}

fn part_path(path: &Path) -> PathBuf {
    let mut s: OsString = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// A file counts as present when its size matches. Downloads are verified
/// before being renamed into place, so hashing every file on every launch
/// isn't needed; we only hash when no size is known.
async fn is_present(job: &DownloadJob) -> Result<bool> {
    let meta = match tokio::fs::metadata(&job.path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).at(&job.path),
    };
    if let Some(size) = job.size {
        return Ok(meta.len() == size);
    }
    if let Some(expected) = &job.sha1 {
        return Ok(sha1_file(&job.path).await?.eq_ignore_ascii_case(expected));
    }
    Ok(true)
}

pub async fn sha1_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path).at(&path)?;
        Ok(hex::encode(Sha1::digest(&bytes)))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn present_checks_size_then_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, b"hello").unwrap();
        let hello_sha1 = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

        let job = |sha1: Option<&str>, size: Option<u64>| DownloadJob {
            url: String::new(),
            path: path.clone(),
            sha1: sha1.map(String::from),
            size,
        };
        assert!(is_present(&job(None, Some(5))).await.unwrap());
        assert!(!is_present(&job(None, Some(6))).await.unwrap());
        assert!(is_present(&job(Some(hello_sha1), None)).await.unwrap());
        assert!(!is_present(&job(Some("00"), None)).await.unwrap());

        let missing = DownloadJob {
            path: dir.path().join("missing"),
            ..job(None, None)
        };
        assert!(!is_present(&missing).await.unwrap());
    }
}
