//! Saved Microsoft accounts.
//!
//! `accounts.json` in the data folder holds each account's Microsoft refresh
//! token, so it lives in the user's own profile folder (like other launchers).
//! Minecraft access tokens are never saved; they're fetched fresh per launch.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::account::{Account, AccountKind};
use crate::auth::{self, AuthError, DeviceLogin, MinecraftSession};
use crate::error::{IoContext, Result, parse_json};
use crate::paths::Paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    uuid: Uuid,
    username: String,
    xuid: String,
    refresh_token: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountsFile {
    #[serde(default)]
    accounts: Vec<StoredAccount>,
    /// `None` means play offline.
    #[serde(default)]
    active: Option<Uuid>,
}

/// What the UI is allowed to see about an account (no tokens).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub uuid: Uuid,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountList {
    pub accounts: Vec<AccountSummary>,
    pub active: Option<Uuid>,
}

pub struct Accounts {
    path: PathBuf,
    http: reqwest::Client,
    /// Serializes read-modify-write of the file.
    lock: Mutex<()>,
}

impl Accounts {
    pub fn new(paths: &Paths) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("BagelTime/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client configuration is valid");
        Self {
            path: paths.root().join("accounts.json"),
            http,
            lock: Mutex::new(()),
        }
    }

    async fn read(&self) -> Result<AccountsFile> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => parse_json(&bytes, "accounts.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountsFile::default()),
            Err(e) => Err(e).at(&self.path),
        }
    }

    async fn write(&self, file: &AccountsFile) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await.at(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(file).expect("accounts serialize");
        tokio::fs::write(&tmp, json).await.at(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        tokio::fs::rename(&tmp, &self.path).await.at(&self.path)
    }

    pub async fn list(&self) -> Result<AccountList> {
        let file = self.read().await?;
        Ok(AccountList {
            accounts: file
                .accounts
                .iter()
                .map(|a| AccountSummary {
                    uuid: a.uuid,
                    username: a.username.clone(),
                })
                .collect(),
            active: file.active,
        })
    }

    pub async fn active(&self) -> Result<Option<Uuid>> {
        Ok(self.read().await?.active)
    }

    /// Step 1 of signing in: get a code for the player to enter at Microsoft.
    pub async fn start_login(&self) -> Result<DeviceLogin> {
        Ok(auth::start_device_login(&self.http).await?)
    }

    /// Step 2: wait for the player, then finish the Xbox/Minecraft chain and
    /// save the account as the active one. Cancel by dropping the future.
    pub async fn finish_login(&self, login: &DeviceLogin) -> Result<AccountSummary> {
        let tokens = auth::poll_device_login(&self.http, login).await?;
        let session = auth::minecraft_login(&self.http, &tokens.access_token).await?;
        let _guard = self.lock.lock().await;
        let mut file = self.read().await?;
        file.accounts.retain(|a| a.uuid != session.uuid);
        file.accounts.push(StoredAccount {
            uuid: session.uuid,
            username: session.username.clone(),
            xuid: session.xuid,
            refresh_token: tokens.refresh_token,
        });
        file.active = Some(session.uuid);
        self.write(&file).await?;
        Ok(AccountSummary {
            uuid: session.uuid,
            username: session.username,
        })
    }

    pub async fn set_active(&self, uuid: Option<Uuid>) -> Result<()> {
        let _guard = self.lock.lock().await;
        let mut file = self.read().await?;
        file.active = uuid.filter(|u| file.accounts.iter().any(|a| a.uuid == *u));
        self.write(&file).await
    }

    pub async fn remove(&self, uuid: Uuid) -> Result<()> {
        let _guard = self.lock.lock().await;
        let mut file = self.read().await?;
        file.accounts.retain(|a| a.uuid != uuid);
        if file.active == Some(uuid) {
            file.active = None;
        }
        self.write(&file).await
    }

    /// A fresh, ready-to-launch session for a saved account. Saves the rotated
    /// refresh token and picks up username changes.
    pub async fn launch_account(&self, uuid: Uuid) -> Result<Account> {
        let stored = self
            .read()
            .await?
            .accounts
            .into_iter()
            .find(|a| a.uuid == uuid)
            .ok_or(AuthError::SessionExpired)?;
        let tokens = auth::refresh(&self.http, &stored.refresh_token).await?;
        let session: MinecraftSession = auth::minecraft_login(&self.http, &tokens.access_token).await?;

        let _guard = self.lock.lock().await;
        let mut file = self.read().await?;
        if let Some(a) = file.accounts.iter_mut().find(|a| a.uuid == uuid) {
            a.refresh_token = tokens.refresh_token;
            a.username = session.username.clone();
            a.xuid = session.xuid.clone();
        }
        self.write(&file).await?;

        Ok(Account {
            username: session.username,
            uuid: session.uuid,
            access_token: session.access_token,
            kind: AccountKind::Microsoft { xuid: session.xuid },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(name: &str, n: u128) -> StoredAccount {
        StoredAccount {
            uuid: Uuid::from_u128(n),
            username: name.into(),
            xuid: "1".into(),
            refresh_token: "secret".into(),
        }
    }

    #[tokio::test]
    async fn switching_and_removing() {
        let tmp = tempfile::tempdir().unwrap();
        let accounts = Accounts::new(&Paths::new(tmp.path()));
        assert!(accounts.list().await.unwrap().accounts.is_empty());

        let (a, b) = (stored("Alex", 1), stored("Steve", 2));
        let (ua, ub) = (a.uuid, b.uuid);
        accounts
            .write(&AccountsFile {
                accounts: vec![a, b],
                active: Some(ua),
            })
            .await
            .unwrap();

        accounts.set_active(Some(ub)).await.unwrap();
        assert_eq!(accounts.active().await.unwrap(), Some(ub));
        // Unknown accounts can't become active.
        accounts.set_active(Some(Uuid::from_u128(99))).await.unwrap();
        assert_eq!(accounts.active().await.unwrap(), None);

        accounts.set_active(Some(ub)).await.unwrap();
        accounts.remove(ub).await.unwrap();
        let list = accounts.list().await.unwrap();
        assert_eq!(list.active, None);
        assert_eq!(list.accounts.len(), 1);
        assert_eq!(list.accounts[0].username, "Alex");
    }

    #[test]
    fn summaries_never_include_tokens() {
        let s = serde_json::to_string(&AccountSummary {
            uuid: Uuid::nil(),
            username: "Alex".into(),
        })
        .unwrap();
        assert!(!s.contains("token"));
    }
}
