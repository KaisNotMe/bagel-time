//! Bagel Time launcher core: everything except the user interface.

pub mod account;
mod assets;
pub mod download;
pub mod error;
pub mod instance;
pub mod java;
pub mod launch;
mod launcher;
pub mod libraries;
pub mod logs;
pub mod meta;
pub mod paths;
pub mod progress;
pub mod rules;
pub mod settings;

pub use account::Account;
pub use error::{Error, Result};
pub use instance::{Instance, InstanceStore};
pub use launcher::{InstalledVersion, LaunchOptions, Launcher};
pub use paths::Paths;
pub use progress::{Progress, ProgressEvent};
pub use settings::Settings;
