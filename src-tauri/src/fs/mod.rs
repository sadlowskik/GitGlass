use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::git::{GitStatus, RepoContext};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryDto {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size_bytes: u64,
    pub modified_ms: Option<u64>,
    pub git_status: Option<GitStatus>,
    pub has_changes: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfoDto {
    pub root: String,
    pub branch: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub is_detached: bool,
    /// Repo-wide, for the commit panel & sync toolbar.
    pub staged_count: u32,
    pub unstaged_count: u32,
    /// `origin` remote URL, if any — lets the UI choose publish vs. open-PR.
    pub remote_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListingDto {
    pub path: String,
    pub parent: Option<String>,
    pub repo: Option<RepoInfoDto>,
    pub entries: Vec<DirEntryDto>,
}

/// List a directory and annotate each entry with git status when the directory
/// lives inside a repository. Git status is computed ONCE per call.
pub fn list_dir(path: &Path) -> AppResult<DirListingDto> {
    let meta = std::fs::metadata(path)?;
    if !meta.is_dir() {
        return Err(AppError::new(
            ErrorKind::NotADirectory,
            "That path isn’t a folder.",
        ));
    }

    let repo = RepoContext::discover(path)?;

    let mut entries = Vec::new();
    for dirent in std::fs::read_dir(path)? {
        let dirent = match dirent {
            Ok(d) => d,
            Err(_) => continue, // Skip entries we transiently can't read.
        };
        let entry_path = dirent.path();
        let file_type = match dirent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let is_symlink = file_type.is_symlink();
        // Follow the symlink for is_dir so linked folders navigate correctly.
        let is_dir = if is_symlink {
            std::fs::metadata(&entry_path)
                .map(|m| m.is_dir())
                .unwrap_or(false)
        } else {
            file_type.is_dir()
        };

        let (size_bytes, modified_ms) = match dirent.metadata() {
            Ok(m) => (if m.is_dir() { 0 } else { m.len() }, modified_ms(&m)),
            Err(_) => (0, None),
        };

        let (git_status, has_changes) = match &repo {
            Some(ctx) => {
                if is_dir {
                    let (s, changed) = ctx.dir_status(&entry_path);
                    (Some(s), changed)
                } else {
                    (Some(ctx.status_of(&entry_path)), false)
                }
            }
            None => (None, false),
        };

        entries.push(DirEntryDto {
            name: dirent.file_name().to_string_lossy().to_string(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            is_symlink,
            size_bytes,
            modified_ms,
            git_status,
            has_changes,
        });
    }

    let repo_dto = repo.map(|r| {
        let (staged, unstaged) = r.counts();
        RepoInfoDto {
            root: r.root.to_string_lossy().to_string(),
            branch: r.branch.clone(),
            ahead: r.ahead.map(|n| n as u32),
            behind: r.behind.map(|n| n as u32),
            is_detached: r.is_detached,
            staged_count: staged as u32,
            unstaged_count: unstaged as u32,
            remote_url: r.origin_url.clone(),
        }
    });

    Ok(DirListingDto {
        path: path.to_string_lossy().to_string(),
        parent: path.parent().map(|p| p.to_string_lossy().to_string()),
        repo: repo_dto,
        entries,
    })
}

pub fn home_dir() -> AppResult<PathBuf> {
    dirs::home_dir()
        .ok_or_else(|| AppError::new(ErrorKind::NotFound, "Couldn’t locate your home folder."))
}

fn modified_ms(m: &std::fs::Metadata) -> Option<u64> {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}
