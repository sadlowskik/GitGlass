use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};

use super::api;

const KEYRING_SERVICE: &str = "GitGlass";
const KEYRING_ACCOUNT: &str = "github-token";

// Scopes we request in the device flow:
//   repo      — create repos, push, open PRs, read PR/issue status
//   read:user — show who's signed in
//   workflow  — REQUIRED to push files under .github/workflows. Without it
//               GitHub rejects the whole push ("refusing to allow an OAuth App
//               to create or update workflow ... without `workflow` scope"),
//               which matters because GitGlass *writes* workflows (Automations).
const SCOPE: &str = "repo read:user workflow";

/// Token lives ONLY in the OS keychain (Windows Credential Manager / macOS
/// Keychain). Never written to config, never logged.
fn entry() -> AppResult<Entry> {
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT).map_err(Into::into)
}

pub fn store_token(token: &str) -> AppResult<()> {
    entry()?.set_password(token)?;
    Ok(())
}

pub fn read_token() -> Option<String> {
    entry().ok()?.get_password().ok()
}

pub fn delete_token() -> AppResult<()> {
    // Deleting a non-existent entry is not an error for us.
    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
    Ok(())
}

fn require_client_id(client_id: &str) -> AppResult<&str> {
    let id = client_id.trim();
    if id.is_empty() {
        return Err(AppError::new(
            ErrorKind::Auth,
            "GitHub sign-in isn’t configured yet.",
        )
        .with_detail("Set VITE_GITHUB_CLIENT_ID (a GitHub OAuth App client id with device flow enabled)."));
    }
    Ok(id)
}

// --- Device flow ------------------------------------------------------------

#[derive(Deserialize)]
struct DeviceCodeResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeDto {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub connected: bool,
    pub login: Option<String>,
    pub avatar_url: Option<String>,
}

impl AuthStatus {
    fn disconnected() -> Self {
        Self {
            connected: false,
            login: None,
            avatar_url: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginPoll {
    /// One of: authorized | pending | slow_down | denied | expired | error
    pub status: String,
    pub auth: Option<AuthStatus>,
    /// Suggested new interval on slow_down.
    pub interval: Option<u64>,
}

/// Step 1: request a device + user code the person enters at github.com.
pub async fn start_login(client_id: &str) -> AppResult<DeviceCodeDto> {
    let id = require_client_id(client_id)?;
    let resp: DeviceCodeResp = api::http()
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .json(&serde_json::json!({ "client_id": id, "scope": SCOPE }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(DeviceCodeDto {
        device_code: resp.device_code,
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        expires_in: resp.expires_in,
        interval: resp.interval,
    })
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: Option<String>,
    error: Option<String>,
    interval: Option<u64>,
}

/// Step 2: poll once for the access token. The frontend calls this on the
/// interval GitHub asked for. On success the token is stored in the keychain
/// and the resolved account is returned.
pub async fn poll_login(client_id: &str, device_code: &str) -> AppResult<LoginPoll> {
    let id = require_client_id(client_id)?;
    let resp: TokenResp = api::http()
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": id,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    if let Some(token) = resp.access_token {
        store_token(&token)?;
        let status = current_status().await;
        return Ok(LoginPoll {
            status: "authorized".into(),
            auth: Some(status),
            interval: None,
        });
    }

    let status = match resp.error.as_deref() {
        Some("authorization_pending") => "pending",
        Some("slow_down") => "slow_down",
        Some("access_denied") => "denied",
        Some("expired_token") => "expired",
        _ => "error",
    };
    Ok(LoginPoll {
        status: status.into(),
        auth: None,
        interval: resp.interval,
    })
}

/// Last successfully-resolved account. A transient network error must not flip
/// the UI to "signed out" when we still hold a valid token — so we fall back to
/// this instead of `disconnected()`.
fn last_good() -> &'static std::sync::Mutex<Option<AuthStatus>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<AuthStatus>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Resolve the current signed-in account from the stored token, if any.
///
/// Distinguishes a *rejected* token (really signed out) from a *transient*
/// failure (network down, GitHub 5xx): the latter keeps us signed in and shows
/// the last-known identity, because the token is still valid.
pub async fn current_status() -> AuthStatus {
    let Some(token) = read_token() else {
        *last_good().lock().unwrap() = None;
        return AuthStatus::disconnected();
    };
    match api::get_user(&token).await {
        Ok(user) => {
            let status = AuthStatus {
                connected: true,
                login: Some(user.login),
                avatar_url: user.avatar_url,
            };
            *last_good().lock().unwrap() = Some(status.clone());
            status
        }
        // A real 401/403 means the token was rejected — genuinely signed out.
        Err(e) if matches!(e.kind, ErrorKind::Auth) => {
            *last_good().lock().unwrap() = None;
            AuthStatus::disconnected()
        }
        // Network/5xx: we still hold a token, so stay signed in. Show the
        // last-known identity if we have one, otherwise connected-without-name.
        Err(_) => last_good().lock().unwrap().clone().unwrap_or(AuthStatus {
            connected: true,
            login: None,
            avatar_url: None,
        }),
    }
}

pub fn sign_out() -> AppResult<()> {
    delete_token()
}

/// Read the stored token or a friendly "please sign in" error. Used by the
/// repo/PR operations that require authentication.
pub fn require_token() -> AppResult<String> {
    read_token().ok_or_else(|| AppError::new(ErrorKind::Auth, "Please sign in to GitHub first."))
}
