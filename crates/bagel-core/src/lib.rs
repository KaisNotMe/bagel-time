//! Bagel Time launcher core: everything except the user interface.

pub mod account;
pub mod accounts;
mod assets;
pub mod auth;
pub mod download;
pub mod error;
mod forge;
pub mod game_files;
pub mod instance;
pub mod java;
pub mod launch;
mod launcher;
pub mod libraries;
pub mod loaders;
pub mod logs;
pub mod meta;
pub mod modrinth;
pub mod cfpack;
pub mod content;
pub mod curseforge;
pub mod mrpack;
pub mod paths;
pub mod progress;
pub mod rules;
pub mod settings;

pub use account::Account;
pub use accounts::Accounts;
pub use error::{Error, Result};
pub use instance::{Instance, InstanceStore};
pub use launcher::{InstalledVersion, LaunchOptions, Launcher};
pub use loaders::{GameVersion, Loader, LoaderVersion};
pub use paths::Paths;
pub use progress::{Progress, ProgressEvent};
pub use settings::Settings;
