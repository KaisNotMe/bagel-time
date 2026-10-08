use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {what}: {source}")]
    Json {
        what: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("checksum mismatch for {url}: expected {expected}, got {actual}")]
    Checksum {
        url: String,
        expected: String,
        actual: String,
    },

    #[error("instance not found: {0}")]
    InstanceNotFound(String),

    #[error("no {0} version chosen for this instance")]
    MissingLoaderVersion(String),

    #[error("unknown Minecraft version: {0}")]
    UnknownVersion(String),

    #[error("no Java runtime '{component}' is available for {platform}")]
    NoJavaRuntime { component: String, platform: String },

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("background task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Attach a path to `std::io::Error`s so error messages say which file failed.
pub(crate) trait IoContext<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|source| Error::Json {
        what: what.to_string(),
        source,
    })
}
