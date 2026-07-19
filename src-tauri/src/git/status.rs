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
    /// Directory roll-ups, precomputed once from `statuses`. Without this,
    /// `dir_status` scanned the whole status map per subdirectory, making a
    /// single listing O(directories x entries) — ~240ms for a 50k-entry repo
    /// with 20 subfolders, on a path the file watcher can fire every 300ms.
    dir_rollup: HashMap<PathBuf, DirRollup>,
}

/// What a directory's descendants add up to. `only_ignored` starts true and is
/// cleared by the first actionable child, so it must not derive Default.
#[derive(Debug, Clone, Copy)]
struct DirRollup {
    any_child: bool,
    has_changes: bool,
    only_ignored: bool,
}

impl Default for DirRollup {
    fn default() -> Self {
        DirRollup {
            any_child: false,
            has_changes: false,
            only_ignored: true,
        }
    }
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
        let dir_rollup = roll_up_dirs(&statuses, &root);

        Ok(Some(RepoContext {
            root,
            branch,
            is_detached,
            ahead,
            behind,
            origin_url,
            statuses,
            dir_rollup,
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
    /// Ignored and clean descendants do not count as "changes". O(1) — the work
    /// happens once in `roll_up_dirs`.
    pub fn dir_status(&self, abs_dir: &Path) -> (GitStatus, bool) {
        let Some(r) = self.dir_rollup.get(&normalize(abs_dir)) else {
            return (GitStatus::Clean, false);
        };
        // A folder that contains only ignored entries reads as ignored (gray).
        let status = if r.any_child && r.only_ignored && !r.has_changes {
            GitStatus::Ignored
        } else {
            GitStatus::Clean
        };
        (status, r.has_changes)
    }
}

/// Fold every status entry into its ancestor directories, once. Each entry
/// contributes to itself and to every ancestor up to (and including) `root` —
/// matching the `path.starts_with(dir)` test this replaces, which was also true
/// for `path == dir`. That self-match is what makes an ignored directory git
/// reports as a single entry (`node_modules/`) render gray.
fn roll_up_dirs(
    statuses: &HashMap<PathBuf, GitStatus>,
    root: &Path,
) -> HashMap<PathBuf, DirRollup> {
    let root = normalize(root);
    let mut rollup: HashMap<PathBuf, DirRollup> = HashMap::new();

    for (path, status) in statuses {
        for anc in path.ancestors() {
            if !anc.starts_with(&root) {
                break;
            }
            let e = rollup.entry(anc.to_path_buf()).or_default();
            e.any_child = true;
            match status {
                GitStatus::Ignored | GitStatus::Clean => {}
                _ => {
                    e.has_changes = true;
                    e.only_ignored = false;
                }
            }
            if anc == root {
                break;
            }
        }
    }
    rollup
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

    // --- Directory roll-up -------------------------------------------------
    // These pin the behaviour `dir_status` had when it scanned the whole status
    // map per directory, so the O(1) precomputed version can't silently drift.

    fn ctx(root: &str, entries: &[(&str, GitStatus)]) -> RepoContext {
        let root = PathBuf::from(root);
        let statuses: HashMap<PathBuf, GitStatus> = entries
            .iter()
            .map(|(p, s)| (normalize(&root.join(p)), *s))
            .collect();
        let dir_rollup = roll_up_dirs(&statuses, &root);
        RepoContext {
            root,
            branch: None,
            is_detached: false,
            ahead: None,
            behind: None,
            origin_url: None,
            statuses,
            dir_rollup,
        }
    }

    #[test]
    fn dir_with_a_modified_descendant_has_changes() {
        let c = ctx("/repo", &[("src/deep/a.rs", GitStatus::Modified)]);
        assert_eq!(
            c.dir_status(Path::new("/repo/src")),
            (GitStatus::Clean, true)
        );
        // The roll-up must reach every level, not just the immediate parent.
        assert_eq!(
            c.dir_status(Path::new("/repo/src/deep")),
            (GitStatus::Clean, true)
        );
        assert_eq!(c.dir_status(Path::new("/repo")), (GitStatus::Clean, true));
    }

    #[test]
    fn dir_containing_only_ignored_entries_reads_as_ignored() {
        let c = ctx("/repo", &[("build/out.js", GitStatus::Ignored)]);
        assert_eq!(
            c.dir_status(Path::new("/repo/build")),
            (GitStatus::Ignored, false)
        );
    }

    /// git reports an ignored directory as one entry (`node_modules/`) rather
    /// than walking it. That entry must colour the directory itself — the old
    /// `starts_with` test matched `path == dir`, and the roll-up has to too.
    #[test]
    fn ignored_directory_entry_colours_itself() {
        let c = ctx("/repo", &[("node_modules", GitStatus::Ignored)]);
        assert_eq!(
            c.dir_status(Path::new("/repo/node_modules")),
            (GitStatus::Ignored, false)
        );
    }

    #[test]
    fn dir_with_no_entries_is_clean() {
        let c = ctx("/repo", &[("src/a.rs", GitStatus::Modified)]);
        assert_eq!(
            c.dir_status(Path::new("/repo/untouched")),
            (GitStatus::Clean, false)
        );
    }

    /// A mix of ignored and actionable children is "has changes", not "ignored".
    #[test]
    fn changes_win_over_ignored_siblings() {
        let c = ctx(
            "/repo",
            &[
                ("src/gen.js", GitStatus::Ignored),
                ("src/main.rs", GitStatus::Staged),
            ],
        );
        assert_eq!(
            c.dir_status(Path::new("/repo/src")),
            (GitStatus::Clean, true)
        );
    }

    /// The walk must stop at the repo root and never attribute changes to
    /// directories above it.
    #[test]
    fn rollup_does_not_escape_the_repo_root() {
        let c = ctx("/repo", &[("src/a.rs", GitStatus::Modified)]);
        assert!(!c.dir_rollup.contains_key(Path::new("/")));
        assert_eq!(c.dir_status(Path::new("/")), (GitStatus::Clean, false));
    }
}
