use std::collections::HashMap;
use std::path::{Path, PathBuf};

use git2::{Repository, Status, StatusOptions};
use serde::Serialize;

use crate::error::AppResult;

/// User-facing git state for a single path. `clean` means tracked & unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitStatus {
    Staged,
    Modified,
    Untracked,
    Ignored,
    Conflict,
    Clean,
}

/// Everything we know about the repository a path lives in, computed once per
/// directory listing. The status map is keyed by ABSOLUTE, normalized path so
/// lookups from the fs layer are cheap.
pub struct RepoContext {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub is_detached: bool,
    /// Commits the local branch is ahead/behind its upstream. None when there
    /// is no upstream tracking branch. Computed locally — no network.
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// URL of the `origin` remote, if one is configured.
    pub origin_url: Option<String>,
    /// Absolute path -> status, for every non-clean/ignored entry git reports.
    statuses: HashMap<PathBuf, GitStatus>,
}

impl RepoContext {
    /// Discover the repo containing `path` and compute its status once.
    /// Returns Ok(None) when `path` is not inside any git repository.
    pub fn discover(path: &Path) -> AppResult<Option<RepoContext>> {
        let repo = match Repository::discover(path) {
            Ok(r) => r,
            // discover() errors when no repo is found — that's a normal case,
            // not a failure we want to surface to the user.
            Err(_) => return Ok(None),
        };

        // Bare repos have no working directory to browse.
        let root = match repo.workdir() {
            Some(w) => w.to_path_buf(),
            None => return Ok(None),
        };

        let (branch, is_detached) = head_info(&repo);
        let (ahead, behind) = ahead_behind(&repo);
        let origin_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()));
        let statuses = collect_statuses(&repo, &root)?;

        Ok(Some(RepoContext {
            root,
            branch,
            is_detached,
            ahead,
            behind,
            origin_url,
            statuses,
        }))
    }

    /// Repo-wide counts for the commit panel: (staged, unstaged). "unstaged"
    /// covers modified + untracked + conflicted working-tree entries. A file
    /// counted as staged is not double-counted here (see `classify`).
    pub fn counts(&self) -> (usize, usize) {
        let mut staged = 0;
        let mut unstaged = 0;
        for s in self.statuses.values() {
            match s {
                GitStatus::Staged => staged += 1,
                GitStatus::Modified | GitStatus::Untracked | GitStatus::Conflict => unstaged += 1,
                _ => {}
            }
        }
        (staged, unstaged)
    }

    /// Status for an exact file path (defaults to Clean when tracked & unchanged).
    pub fn status_of(&self, abs: &Path) -> GitStatus {
        self.statuses
            .get(&normalize(abs))
            .copied()
            .unwrap_or(GitStatus::Clean)
    }

    /// Roll-up for a directory: does any descendant have an actionable change?
    /// Ignored and clean descendants do not count as "changes".
    pub fn dir_status(&self, abs_dir: &Path) -> (GitStatus, bool) {
        let dir = normalize(abs_dir);
        let mut has_changes = false;
        let mut only_ignored = true;
        let mut any_child = false;

        for (path, status) in &self.statuses {
            if !path.starts_with(&dir) {
                continue;
            }
            any_child = true;
            match status {
                GitStatus::Ignored => {}
                GitStatus::Clean => {}
                _ => {
                    has_changes = true;
                    only_ignored = false;
                }
            }
        }

        // A folder that contains only ignored entries reads as ignored (gray).
        let status = if any_child && only_ignored && !has_changes {
            GitStatus::Ignored
        } else {
            GitStatus::Clean
        };
        (status, has_changes)
    }
}

/// Compute how far the current branch is ahead/behind its upstream, using only
/// local refs (no fetch). Returns (None, None) when there's no upstream.
fn ahead_behind(repo: &Repository) -> (Option<usize>, Option<usize>) {
    // Detached HEAD or unborn branch has no meaningful upstream comparison.
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return (None, None),
    };
    if !head.is_branch() {
        return (None, None);
    }
    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return (None, None),
    };
    let branch = git2::Branch::wrap(head);
    let upstream = match branch.upstream() {
        Ok(u) => u,
        Err(_) => return (None, None), // no tracking branch configured
    };
    let upstream_oid = match upstream.get().target() {
        Some(oid) => oid,
        None => return (None, None),
    };
    match repo.graph_ahead_behind(local_oid, upstream_oid) {
        Ok((ahead, behind)) => (Some(ahead), Some(behind)),
        Err(_) => (None, None),
    }
}

fn head_info(repo: &Repository) -> (Option<String>, bool) {
    match repo.head() {
        Ok(head) => {
            let detached = repo.head_detached().unwrap_or(false);
            let name = head.shorthand().map(|s| s.to_string());
            (name, detached)
        }
        // Unborn branch (fresh repo, no commits yet).
        Err(_) => (Some("main".to_string()), false),
    }
}

fn collect_statuses(repo: &Repository, root: &Path) -> AppResult<HashMap<PathBuf, GitStatus>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(true)
        // Report an ignored directory once instead of walking into it (keeps
        // node_modules etc. from exploding the status list).
        .recurse_ignored_dirs(false)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut map = HashMap::with_capacity(statuses.len());

    for entry in statuses.iter() {
        let Some(rel) = entry.path() else { continue };
        let abs = normalize(&root.join(rel));
        map.insert(abs, classify(entry.status()));
    }
    Ok(map)
}

/// Map git2 status bits to a single user-facing state, in priority order.
fn classify(s: Status) -> GitStatus {
    if s.is_conflicted() {
        return GitStatus::Conflict;
    }
    // Anything staged in the index is "ready to save".
    const STAGED: Status = Status::from_bits_truncate(
        Status::INDEX_NEW.bits()
            | Status::INDEX_MODIFIED.bits()
            | Status::INDEX_DELETED.bits()
            | Status::INDEX_RENAMED.bits()
            | Status::INDEX_TYPECHANGE.bits(),
    );
    if s.intersects(STAGED) {
        return GitStatus::Staged;
    }
    const WT_MODIFIED: Status = Status::from_bits_truncate(
        Status::WT_MODIFIED.bits()
            | Status::WT_DELETED.bits()
            | Status::WT_RENAMED.bits()
            | Status::WT_TYPECHANGE.bits(),
    );
    if s.intersects(WT_MODIFIED) {
        return GitStatus::Modified;
    }
    if s.contains(Status::WT_NEW) {
        return GitStatus::Untracked;
    }
    if s.contains(Status::IGNORED) {
        return GitStatus::Ignored;
    }
    GitStatus::Clean
}

/// Normalize a path for stable map keys: strip verbatim prefixes and lowercase
/// the drive letter on Windows so lookups from the fs layer always match.
fn normalize(p: &Path) -> PathBuf {
    // dunce-free minimal normalization; full canonicalization would hit the
    // filesystem per call, which we avoid in hot paths.
    let s = p
        .to_string_lossy()
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    let trimmed = s.strip_prefix(r"\\?\").unwrap_or(&s);
    PathBuf::from(trimmed.trim_end_matches(['/', '\\']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_conflict_wins() {
        let s = Status::CONFLICTED | Status::WT_MODIFIED;
        assert_eq!(classify(s), GitStatus::Conflict);
    }

    #[test]
    fn classify_staged_over_modified_wt() {
        // Staged addition that was then modified in the working tree still
        // reads as "staged" (the actionable state is the index entry).
        let s = Status::INDEX_NEW | Status::WT_MODIFIED;
        assert_eq!(classify(s), GitStatus::Staged);
    }

    #[test]
    fn classify_plain_modified() {
        assert_eq!(classify(Status::WT_MODIFIED), GitStatus::Modified);
    }

    #[test]
    fn classify_untracked() {
        assert_eq!(classify(Status::WT_NEW), GitStatus::Untracked);
    }

    #[test]
    fn classify_ignored() {
        assert_eq!(classify(Status::IGNORED), GitStatus::Ignored);
    }

    #[test]
    fn classify_clean_when_empty() {
        assert_eq!(classify(Status::CURRENT), GitStatus::Clean);
    }
}
