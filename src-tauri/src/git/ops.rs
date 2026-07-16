use std::path::{Path, PathBuf};

use git2::{IndexAddOption, Repository};
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};

/// The name/email Git will stamp on commits, read from global config. Either
/// field may be missing — a first-time user often has neither.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Read the user's configured Git identity (global config), if any.
pub fn git_identity() -> Identity {
    let cfg = git2::Config::open_default().ok();
    let get = |key: &str| {
        cfg.as_ref()
            .and_then(|c| c.get_string(key).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    Identity {
        name: get("user.name"),
        email: get("user.email"),
    }
}

/// Find Git repositories nested *inside* `root` (excluding root's own `.git`).
/// Git can't track a repo within a repo, so these must be surfaced clearly
/// rather than failing with a cryptic libgit2 error.
pub fn find_embedded_repos(root: &Path) -> Vec<String> {
    const SKIP: &[&str] = &[
        "node_modules",
        "target",
        "dist",
        "build",
        ".venv",
        "venv",
        "__pycache__",
    ];
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" {
                // The root's own .git is fine; anything deeper is embedded.
                if dir != root {
                    if let Ok(rel) = dir.strip_prefix(root) {
                        found.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
                continue;
            }
            if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                continue;
            }
            stack.push(path);
        }
    }
    found.sort();
    found
}

/// Turn a plain folder into a Git repository with an initial commit — entirely
/// local, nothing uploaded, no GitHub account needed.
///
/// `name`/`email` are stored in the new repo's config when supplied, so a user
/// who has never configured Git can still commit. The staged content is
/// secret-scanned exactly like any other commit; if anything is found we roll
/// back the `.git` directory we created so the folder is left untouched.
pub fn init_repo(
    path: &Path,
    name: Option<&str>,
    email: Option<&str>,
    allow_secrets: bool,
) -> AppResult<()> {
    // Refuse if this folder (or a parent) is already tracked.
    if Repository::discover(path).is_ok() {
        return Err(AppError::new(
            ErrorKind::Git,
            "This folder is already tracked by Git.",
        ));
    }

    let repo = Repository::init(path)?;
    // Start on `main` rather than whatever git's default happens to be.
    let _ = repo.set_head("refs/heads/main");

    // Persist the identity locally so future commits in this repo work too.
    if let (Some(n), Some(e)) = (name, email) {
        if !n.trim().is_empty() && !e.trim().is_empty() {
            let mut cfg = repo.config()?;
            cfg.set_str("user.name", n.trim())?;
            cfg.set_str("user.email", e.trim())?;
        }
    }

    {
        let mut index = repo.index()?;
        // Stage everything not covered by .gitignore (like `git add -A`).
        index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;
    }

    if !allow_secrets {
        let findings = crate::secret_scan::scan_staged(path)?;
        if !findings.is_empty() {
            // Roll back: drop handles first so Windows releases the files.
            drop(repo);
            let _ = std::fs::remove_dir_all(path.join(".git"));
            return Err(AppError::new(
                ErrorKind::SecretsFound,
                "This folder looks like it contains secrets. Review them before tracking it with Git.",
            )
            .with_detail(format!("{} suspected secret(s) found.", findings.len())));
        }
    }

    let signature = author_signature(&repo)?;
    let mut index = repo.index()?;
    let tree = repo.find_tree(index.write_tree()?)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )?;
    Ok(())
}

/// Open the repository that contains `path`. Every write op re-opens rather than
/// holding a long-lived handle, so we never operate on stale index state.
pub fn open_repo(path: &Path) -> AppResult<Repository> {
    Repository::discover(path)
        .map_err(|_| AppError::new(ErrorKind::NotARepo, "This folder isn’t a Git repository."))
}

/// Normalize a path for prefix comparison: unify separators and strip Windows
/// verbatim (`\\?\`) prefixes and any trailing separator. We intentionally do
/// NOT touch case — paths from the UI share the same navigation origin as the
/// repo workdir, so their case already matches.
fn norm(p: &Path) -> String {
    let s = p
        .to_string_lossy()
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    s.strip_prefix(r"\\?\")
        .unwrap_or(&s)
        .trim_end_matches(['/', '\\'])
        .to_string()
}

/// Convert absolute paths from the UI into repo-relative pathspecs. Robust to
/// verbatim prefixes and separator differences, and guards against false prefix
/// matches (e.g. `/repo` must not match `/repo2/file`). Paths outside the
/// working directory are skipped rather than mis-staged.
fn relativize(repo: &Repository, paths: &[String]) -> AppResult<Vec<PathBuf>> {
    let workdir = repo.workdir().ok_or_else(|| {
        AppError::new(
            ErrorKind::NotARepo,
            "This repository has no working folder.",
        )
    })?;
    let wnorm = norm(workdir);
    let mut out = Vec::with_capacity(paths.len());

    for p in paths {
        let pnorm = norm(Path::new(p));
        let Some(rest) = pnorm.strip_prefix(&wnorm) else {
            continue; // not under the working directory
        };
        let rest = rest.trim_start_matches(['/', '\\']);
        if rest.is_empty() {
            // The repo root itself -> stage everything.
            out.push(PathBuf::from("*"));
        } else if pnorm.len() > wnorm.len() && !pnorm[wnorm.len()..].starts_with(['/', '\\']) {
            // False prefix (e.g. `/repo` vs `/repo2`): the char after the
            // matched prefix isn't a separator, so this path is not inside.
            continue;
        } else {
            out.push(PathBuf::from(rest));
        }
    }
    Ok(out)
}

/// Convert a single absolute path to a repo-relative, forward-slash pathspec
/// (or None if it's outside the working directory). Used by the diff viewer.
pub(crate) fn to_repo_relative(repo: &Repository, abs: &str) -> Option<String> {
    relativize(repo, std::slice::from_ref(&abs.to_string()))
        .ok()?
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// Stage the given paths. Uses `add_all` semantics (like `git add -A <path>`),
/// so new, modified, AND deleted files are staged correctly.
pub fn stage_paths(repo_path: &Path, paths: &[String]) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let rel = relativize(&repo, paths)?;
    if rel.is_empty() {
        return Ok(());
    }
    let mut index = repo.index()?;
    index.add_all(rel.iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    Ok(())
}

/// Unstage the given paths — reset their index entry back to HEAD without
/// touching the working tree. On an unborn branch (no commits yet) there is no
/// HEAD to reset to, so we remove the entries from the index instead.
pub fn unstage_paths(repo_path: &Path, paths: &[String]) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let rel = relativize(&repo, paths)?;
    if rel.is_empty() {
        return Ok(());
    }

    match repo.head() {
        Ok(head) => {
            let obj = head.peel(git2::ObjectType::Commit)?;
            repo.reset_default(Some(&obj), rel.iter())?;
        }
        Err(_) => {
            // Unborn branch: nothing to reset to, just drop the staged entries.
            let mut index = repo.index()?;
            for r in &rel {
                // Ignore "not found" so unstaging an already-unstaged path is a no-op.
                let _ = index.remove_path(r);
            }
            index.write()?;
        }
    }
    Ok(())
}

/// Create a commit from the currently staged index. Validates that (a) an
/// identity is configured, (b) the message is non-empty, and (c) there is
/// actually something staged — each maps to a distinct, friendly error.
pub fn commit(repo_path: &Path, message: &str) -> AppResult<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(
            ErrorKind::Git,
            "Please write a commit message first.",
        ));
    }

    let repo = open_repo(repo_path)?;
    let signature = author_signature(&repo)?;

    let mut index = repo.index()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None, // initial commit
    };

    // Guard: nothing staged means the new tree equals the parent's tree.
    if let Some(parent) = &parent_commit {
        if parent.tree_id() == tree_oid {
            return Err(AppError::new(
                ErrorKind::NothingToCommit,
                "There’s nothing staged to save yet.",
            ));
        }
    } else if tree.is_empty() {
        return Err(AppError::new(
            ErrorKind::NothingToCommit,
            "There’s nothing staged to save yet.",
        ));
    }

    let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
    let oid = repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        trimmed,
        &tree,
        &parents,
    )?;
    Ok(oid.to_string())
}

/// Append the given paths to the repo's .gitignore (creating it if needed) and
/// unstage them, so a leaked file both stops being tracked and won't be
/// committed. Patterns already present are not duplicated.
pub fn ignore_paths(repo_path: &Path, paths: &[String]) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let rel = relativize(&repo, paths)?;
    if rel.is_empty() {
        return Ok(());
    }
    let workdir = repo.workdir().ok_or_else(|| {
        AppError::new(
            ErrorKind::NotARepo,
            "This repository has no working folder.",
        )
    })?;

    let gitignore = workdir.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(|l| l.trim().to_string()).collect();

    let mut appended = existing;
    if !appended.is_empty() && !appended.ends_with('\n') {
        appended.push('\n');
    }
    for r in &rel {
        // Store gitignore patterns with forward slashes (git's convention).
        let pattern = r.to_string_lossy().replace('\\', "/");
        if pattern == "*" || lines.iter().any(|l| l == &pattern) {
            continue;
        }
        appended.push_str(&pattern);
        appended.push('\n');
        lines.push(pattern);
    }
    std::fs::write(&gitignore, appended)?;

    // Drop the now-ignored files from the index so they won't be committed.
    let mut index = repo.index()?;
    for r in &rel {
        let _ = index.remove_all([r].iter(), None);
    }
    index.write()?;
    Ok(())
}

/// Resolve the committer identity from git config, returning a specific error
/// (not a raw git message) when it isn't set so the UI can prompt for it.
fn author_signature(repo: &Repository) -> AppResult<git2::Signature<'static>> {
    repo.signature().map_err(|_| {
        AppError::new(
            ErrorKind::NoIdentity,
            "GitGlass needs a name and email to save your work.",
        )
        .with_detail("Set user.name and user.email in Git config.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::fs;
    use tempfile::TempDir;

    /// Init a repo with a committer identity and return its dir. `TempDir` is
    /// returned too so it isn't dropped (which would delete the directory).
    fn repo_with_identity() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test User").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    fn abs(root: &Path, name: &str) -> String {
        root.join(name).to_string_lossy().to_string()
    }

    #[test]
    fn stage_then_commit_roundtrip() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("hello.txt"), "hi").unwrap();

        stage_paths(&root, &[abs(&root, "hello.txt")]).unwrap();
        let oid = commit(&root, "first commit").unwrap();
        assert!(!oid.is_empty());

        // HEAD commit carries our message and tree contains the file.
        let repo = Repository::open(&root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "first commit");
        assert!(head.tree().unwrap().get_name("hello.txt").is_some());
    }

    #[test]
    fn commit_with_nothing_staged_is_specific_error() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("untracked.txt"), "x").unwrap();

        let err = commit(&root, "nope").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::NothingToCommit));
    }

    #[test]
    fn empty_message_is_rejected_before_touching_git() {
        let (_d, root) = repo_with_identity();
        let err = commit(&root, "   ").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Git));
    }

    #[test]
    fn unstage_removes_from_index() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("a.txt"), "a").unwrap();

        stage_paths(&root, &[abs(&root, "a.txt")]).unwrap();
        unstage_paths(&root, &[abs(&root, "a.txt")]).unwrap();

        // With nothing staged, a commit must now fail as NothingToCommit.
        let err = commit(&root, "should fail").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::NothingToCommit));
    }

    #[test]
    fn init_repo_makes_a_local_repo_with_first_commit() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("notes.txt"), "hello").unwrap();

        init_repo(
            dir.path(),
            Some("Local User"),
            Some("me@example.com"),
            false,
        )
        .unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap();
        assert_eq!(head.shorthand().unwrap(), "main");
        let commit = head.peel_to_commit().unwrap();
        assert_eq!(commit.message().unwrap(), "Initial commit");
        assert_eq!(commit.author().name().unwrap(), "Local User");
        assert!(commit.tree().unwrap().get_name("notes.txt").is_some());
        // No remote — this is purely local.
        assert!(repo.find_remote("origin").is_err());
    }

    #[test]
    fn init_repo_refuses_an_existing_repo() {
        let (_d, root) = init_repo_dir();
        let err = init_repo(&root, Some("A"), Some("a@b.c"), false).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Git));
    }

    /// Helper: a directory that is already a git repo.
    fn init_repo_dir() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();
        let root = dir.path().to_path_buf();
        (dir, root)
    }

    #[test]
    fn init_repo_blocks_secrets_and_rolls_back() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("cfg.txt"), "aws_key=AKIA1234567890ABCDEF").unwrap();

        let err = init_repo(dir.path(), Some("A"), Some("a@b.c"), false).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::SecretsFound));
        // Rollback: the folder must be left untouched (no .git).
        assert!(
            !dir.path().join(".git").exists(),
            "a blocked init must not leave a half-made repo behind"
        );

        // With the override it succeeds.
        init_repo(dir.path(), Some("A"), Some("a@b.c"), true).unwrap();
        assert!(dir.path().join(".git").exists());
    }

    #[test]
    fn relativize_rejects_false_prefix_sibling() {
        // `<tmp>/repo` and a sibling `<tmp>/repo2` must not be confused.
        let dir = TempDir::new().unwrap();
        let repo_dir = dir.path().join("repo");
        fs::create_dir(&repo_dir).unwrap();
        let repo = Repository::init(&repo_dir).unwrap();

        let sibling_file = dir.path().join("repo2").join("f.txt");
        let rels = relativize(&repo, &[sibling_file.to_string_lossy().to_string()]).unwrap();
        assert!(
            rels.is_empty(),
            "sibling path must not be treated as inside the repo"
        );
    }

    #[test]
    fn relativize_repo_root_becomes_match_all() {
        let (_d, root) = repo_with_identity();
        let repo = Repository::open(&root).unwrap();
        let rels = relativize(&repo, &[root.to_string_lossy().to_string()]).unwrap();
        assert_eq!(rels, vec![PathBuf::from("*")]);
    }
}
