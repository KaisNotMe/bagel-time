use md5::{Digest, Md5};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Account {
    pub username: String,
    pub uuid: Uuid,
    pub access_token: String,
    pub kind: AccountKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountKind {
    /// Singleplayer and offline-mode servers only.
    Offline,
    Microsoft { xuid: String },
}

impl Account {
    pub fn offline(username: &str) -> Self {
        Self {
            username: username.to_string(),
            uuid: offline_uuid(username),
            access_token: "0".to_string(),
            kind: AccountKind::Offline,
        }
    }

    pub fn user_type(&self) -> &'static str {
        match self.kind {
            AccountKind::Offline => "legacy",
            AccountKind::Microsoft { .. } => "msa",
        }
    }

    pub fn xuid(&self) -> &str {
        match &self.kind {
            AccountKind::Offline => "0",
            AccountKind::Microsoft { xuid } => xuid,
        }
    }
}

/// Minecraft usernames: 3-16 characters, letters, digits and underscores.
pub fn is_valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Same UUID vanilla servers assign offline players, i.e. Java's
/// `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`.
pub fn offline_uuid(username: &str) -> Uuid {
    let hash = Md5::digest(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash);
    uuid::Builder::from_md5_bytes(bytes).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn usernames() {
        assert!(is_valid_username("Bagel_Fan42"));
        assert!(!is_valid_username("ab"));
        assert!(!is_valid_username("has space"));
        assert!(!is_valid_username("waytoolongusername1"));
    }
}
