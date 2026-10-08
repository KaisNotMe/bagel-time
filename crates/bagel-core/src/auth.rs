//! Microsoft account sign-in for Minecraft: Java Edition.
//!
//! The chain is: Microsoft OAuth (device code flow) -> Xbox Live -> XSTS ->
//! Minecraft services -> Minecraft profile. Each step trades the previous
//! token for the next one.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// Bagel Time's Azure app registration. Not a secret: desktop apps can't keep
/// one, which is why the app is registered as a public client.
pub const DEFAULT_CLIENT_ID: &str = "a6ed75c2-f98d-4993-a110-bb39e4aab79b";

const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";

/// `BAGEL_MSA_CLIENT_ID` overrides the built-in ID, e.g. for forks.
pub fn client_id() -> String {
    std::env::var("BAGEL_MSA_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("The sign-in code expired before it was used. Try again.")]
    Expired,
    #[error("Sign-in was declined.")]
    Declined,
    #[error("This Microsoft account has no Xbox profile yet. Sign in once at xbox.com to create one, then try again.")]
    NoXboxProfile,
    #[error("Xbox Live isn't available in this account's country or region.")]
    BlockedRegion,
    #[error("This account needs adult verification on xbox.com before it can play.")]
    AdultVerification,
    #[error("This is a child account. An adult needs to add it to a Microsoft family group before it can play.")]
    ChildAccount,
    #[error("Mojang hasn't approved Bagel Time for Minecraft sign-in yet. Offline mode still works in the meantime.")]
    AppNotApproved,
    #[error("This Microsoft account doesn't own Minecraft: Java Edition.")]
    NoMinecraft,
    #[error("Your saved sign-in has expired. Remove the account and sign in again.")]
    SessionExpired,
    #[error("Microsoft sign-in failed: {0}")]
    Other(String),
    #[error("network error during sign-in: {0}")]
    Http(#[from] reqwest::Error),
}

type AuthResult<T> = std::result::Result<T, AuthError>;

/// What the player needs to finish signing in on another page or device.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLogin {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(skip)]
    device_code: String,
    #[serde(skip)]
    interval: u64,
}

#[derive(Debug, Clone)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

/// Result of the full sign-in chain: everything the game needs.
#[derive(Debug, Clone)]
pub struct MinecraftSession {
    pub access_token: String,
    pub uuid: Uuid,
    pub username: String,
    pub xuid: String,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub async fn start_device_login(http: &reqwest::Client) -> AuthResult<DeviceLogin> {
    let resp = http
        .post(format!("{AUTHORITY}/devicecode"))
        .form(&[("client_id", client_id().as_str()), ("scope", SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        let body: TokenResponse = resp.json().await?;
        return Err(AuthError::Other(describe(&body)));
    }
    let r: DeviceCodeResponse = resp.json().await?;
    Ok(DeviceLogin {
        user_code: r.user_code,
        verification_uri: r.verification_uri,
        expires_in: r.expires_in,
        device_code: r.device_code,
        interval: r.interval.unwrap_or(5).max(1),
    })
}

/// Wait until the player finishes signing in. Cancel by dropping the future.
pub async fn poll_device_login(http: &reqwest::Client, login: &DeviceLogin) -> AuthResult<MsTokens> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(login.expires_in);
    let mut interval = login.interval;
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if tokio::time::Instant::now() >= deadline {
            return Err(AuthError::Expired);
        }
        let body: TokenResponse = http
            .post(format!("{AUTHORITY}/token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id().as_str()),
                ("device_code", login.device_code.as_str()),
            ])
            .send()
            .await?
            .json()
            .await?;
        match body.error.as_deref() {
            None => return tokens(body),
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            Some("expired_token") | Some("code_expired") => return Err(AuthError::Expired),
            Some("authorization_declined") | Some("access_denied") => return Err(AuthError::Declined),
            Some(_) => return Err(AuthError::Other(describe(&body))),
        }
    }
}

/// Trade a saved refresh token for fresh tokens. Microsoft may rotate the
/// refresh token, so callers must store the new one.
pub async fn refresh(http: &reqwest::Client, refresh_token: &str) -> AuthResult<MsTokens> {
    let body: TokenResponse = http
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id().as_str()),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?
        .json()
        .await?;
    match body.error.as_deref() {
        None => tokens(body),
        Some("invalid_grant") => Err(AuthError::SessionExpired),
        Some(_) => Err(AuthError::Other(describe(&body))),
    }
}

fn tokens(body: TokenResponse) -> AuthResult<MsTokens> {
    match (body.access_token, body.refresh_token) {
        (Some(access_token), Some(refresh_token)) => Ok(MsTokens {
            access_token,
            refresh_token,
        }),
        _ => Err(AuthError::Other("Microsoft didn't return a token".into())),
    }
}

fn describe(body: &TokenResponse) -> String {
    match (&body.error, &body.error_description) {
        (_, Some(d)) => d.lines().next().unwrap_or(d).to_string(),
        (Some(e), None) => e.clone(),
        (None, None) => "unknown error".into(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxResponse {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<XuiClaim>,
}

#[derive(Deserialize)]
struct XuiClaim {
    uhs: String,
    xid: Option<String>,
}

#[derive(Deserialize)]
struct XstsError {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
}

#[derive(Deserialize)]
struct McLoginResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct McProfile {
    id: String,
    name: String,
}

/// Xbox Live -> XSTS -> Minecraft, starting from a Microsoft access token.
pub async fn minecraft_login(http: &reqwest::Client, ms_access_token: &str) -> AuthResult<MinecraftSession> {
    let xbl: XboxResponse = http
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let resp = http
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let err: XstsError = resp.json().await.unwrap_or(XstsError { xerr: None });
        return Err(match err.xerr {
            Some(2148916233) => AuthError::NoXboxProfile,
            Some(2148916235) => AuthError::BlockedRegion,
            Some(2148916236) | Some(2148916237) => AuthError::AdultVerification,
            Some(2148916238) => AuthError::ChildAccount,
            other => AuthError::Other(format!("Xbox Live refused the sign-in (code {other:?})")),
        });
    }
    let xsts: XboxResponse = resp.error_for_status()?.json().await?;
    let claim = xsts
        .display_claims
        .xui
        .first()
        .ok_or_else(|| AuthError::Other("Xbox Live returned no user".into()))?;

    let resp = http
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={};{}", claim.uhs, xsts.token) }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(AuthError::AppNotApproved);
    }
    let mc: McLoginResponse = resp.error_for_status()?.json().await?;

    let resp = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&mc.access_token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AuthError::NoMinecraft);
    }
    let profile: McProfile = resp.error_for_status()?.json().await?;
    let uuid = Uuid::parse_str(&profile.id)
        .map_err(|_| AuthError::Other(format!("unexpected profile id {}", profile.id)))?;

    Ok(MinecraftSession {
        access_token: mc.access_token,
        uuid,
        username: profile.name,
        xuid: claim.xid.clone().unwrap_or_else(|| "0".to_string()),
    })
}
