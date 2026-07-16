use serde::Serialize;

/// The ONLY error shape that crosses the IPC boundary. We deliberately never
/// forward a raw `git2::Error` or `std::io::Error` string to the UI as the
/// primary message — `message` is always already user-friendly. Raw context
/// goes into `detail`, which the UI hides behind a disclosure.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotFound,
    Permission,
    NotADirectory,
    NotARepo,
    /// Nothing was staged when a commit was attempted.
    NothingToCommit,
    /// No committer identity configured (user.name / user.email).
    NoIdentity,
    /// No upstream/tracking branch is set for push/pull.
    NoUpstream,
    /// Remote rejected a push because local is behind ("pull first").
    NonFastForward,
    /// Authentication with the remote failed / no credentials available.
    Auth,
    /// The staged changes contain suspected secrets; commit was blocked.
    SecretsFound,
    Git,
    Io,
    #[allow(dead_code)]
    Unknown,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind as K;
        let (kind, msg) = match e.kind() {
            K::NotFound => (ErrorKind::NotFound, "That folder couldn’t be found."),
            K::PermissionDenied => (ErrorKind::Permission, "GitGlass can’t open this folder."),
            _ => (ErrorKind::Io, "Something went wrong reading that folder."),
        };
        AppError::new(kind, msg).with_detail(e.to_string())
    }
}

impl From<git2::Error> for AppError {
    fn from(e: git2::Error) -> Self {
        // git2 errors are developer-oriented; map the common ones to friendly
        // language and keep the raw text only in `detail`.
        AppError::new(
            ErrorKind::Git,
            "GitGlass couldn’t read this repository’s Git data.",
        )
        .with_detail(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::new(
            ErrorKind::Io,
            "GitGlass couldn’t reach GitHub. Check your connection and try again.",
        )
        .with_detail(e.to_string())
    }
}

impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        AppError::new(
            ErrorKind::Io,
            "GitGlass couldn’t access the secure credential store.",
        )
        .with_detail(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
