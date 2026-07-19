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

/// Save the committer identity to the user's **global** Git config, creating the
/// config file if they've never had one.
///
/// Global, not per-repo: "who you are" isn't a property of one project, and a
/// first-time user who fixed this in one repo would just hit it again in the
/// next. Goes through git2 rather than `git config`, per the no-CLI rule — the
/// whole point is that the user never opens a terminal.
pub fn set_git_identity(name: &str, email: &str) -> AppResult<()> {
    let (name, email) = validate_identity(name, email)?;
    write_identity(&mut global_config()?, &name, &email)
}

/// Reject the input that would otherwise produce commits nobody can attribute.
/// Kept separate from the write so it's testable without touching the real
/// user's `~/.gitconfig`.
fn validate_identity(name: &str, email: &str) -> AppResult<(String, String)> {
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() {
        return Err(AppError::new(
            ErrorKind::NoIdentity,
            "Please enter a name to sign your work with.",
        ));
    }
    // Deliberately loose: Git itself accepts almost anything here, so this only
    // catches obvious typos rather than trying to be an RFC 5322 validator.
    let plausible = match email.split_once('@') {
        Some((local, domain)) => !local.is_empty() && !domain.is_empty(),
        None => false,
    };
    if !plausible {
        return Err(AppError::new(
            ErrorKind::NoIdentity,
            "Please enter an email address, like you@example.com.",
        ));
    }
    Ok((name.to_string(), email.to_string()))
}

fn write_identity(cfg: &mut git2::Config, name: &str, email: &str) -> AppResult<()> {
    cfg.set_str("user.name", name)?;
    cfg.set_str("user.email", email)?;
    Ok(())
}

/// The writable global (`~/.gitconfig`) config level.
fn global_config() -> AppResult<git2::Config> {
    // The normal path: libgit2 already knows the global level.
    if let Ok(cfg) = git2::Config::open_default() {
        if let Ok(global) = cfg.open_level(git2::ConfigLevel::Global) {
            return Ok(global);
        }
    }
    // A machine that has never run git has no ~/.gitconfig, and there is no
    // global level to open — name the file ourselves so the first save creates it.
    let path = git2::Config::find_global()
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".gitconfig")))
        .ok_or_else(|| {
            AppError::new(
                ErrorKind::Io,
                "GitGlass couldn’t find your home folder to save your Git settings.",
            )
        })?;
    git2::Config::open(&path).map_err(Into::into)
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

/// Point `refs/remotes/<remote>/<branch>` at the branch's current commit and
/// set the branch to track it, so "N ahead / M behind" works after a push.
/// libgit2's push doesn't create the remote-tracking ref the way the git CLI
/// does, so we do it ourselves.
///
/// Best-effort by design: the push has already succeeded by the time this runs,
/// and failing bookkeeping must never turn a successful push into an error.
pub(crate) fn set_tracking(repo: &Repository, remote: &str, branch: &str, reflog_msg: &str) {
    let Ok(oid) = repo.refname_to_id(&format!("refs/heads/{branch}")) else {
        return;
    };
    let _ = repo.reference(
        &format!("refs/remotes/{remote}/{branch}"),
        oid,
        true,
        reflog_msg,
    );
    if let Ok(mut local) = repo.find_branch(branch, git2::BranchType::Local) {
        let _ = local.set_upstream(Some(&format!("{remote}/{branch}")));
    }
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

/// Discard uncommitted changes to the given paths, restoring them to their HEAD
/// state. This DELIBERATELY overwrites working-tree edits and restores deleted
/// files — it's the "throw away my changes" action, so the UI must confirm first.
///
/// Reverts both staged and unstaged modifications. Untracked files are left
/// alone (they have no HEAD version to restore to). On an unborn branch there's
/// no HEAD, so there's nothing tracked to discard.
pub fn discard_paths(repo_path: &Path, paths: &[String]) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    let rel = relativize(&repo, paths)?;
    if rel.is_empty() {
        return Ok(());
    }

    let head = repo.head().map_err(|_| {
        AppError::new(
            ErrorKind::Git,
            "There are no saved versions to restore to yet.",
        )
    })?;

    // Unstage first (so a staged edit is undone too), then force the working
    // tree back to HEAD for just these paths.
    let obj = head.peel(git2::ObjectType::Commit)?;
    let _ = repo.reset_default(Some(&obj), rel.iter());

    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    for r in &rel {
        checkout.path(r);
    }
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}

/// Amend the most recent commit with the currently staged tree and (if
/// non-empty) a new message. Refuses when the commit is already on the remote,
/// since amending rewrites history that others may have pulled.
pub fn amend_commit(repo_path: &Path, message: &str) -> AppResult<String> {
    let repo = open_repo(repo_path)?;
    let head = repo
        .head()
        .map_err(|_| AppError::new(ErrorKind::Git, "There’s no commit to amend yet."))?;
    let commit = head.peel_to_commit()?;

    if commit_is_pushed(&repo, commit.id()) {
        return Err(AppError::new(
            ErrorKind::Git,
            "This commit is already on the remote. Amending would rewrite shared history — make a new commit instead.",
        ));
    }

    let signature = author_signature(&repo)?;
    let mut index = repo.index()?;
    let tree = repo.find_tree(index.write_tree()?)?;

    let trimmed = message.trim();
    let new_message = if trimmed.is_empty() {
        commit.message().unwrap_or("").to_string()
    } else {
        trimmed.to_string()
    };

    let oid = commit.amend(
        Some("HEAD"),
        Some(&signature),
        Some(&signature),
        None,
        Some(&new_message),
        Some(&tree),
    )?;
    Ok(oid.to_string())
}

/// Whether `commit` is already reachable from the current branch's upstream
/// (i.e. has been pushed). Conservative: if there's no upstream we treat it as
/// unpushed. Uses the last-fetched remote-tracking ref, so it can only ever be
/// stale in the *safe* direction after a fetch.
fn commit_is_pushed(repo: &Repository, commit: git2::Oid) -> bool {
    let Ok(head) = repo.head() else {
        return false;
    };
    if !head.is_branch() {
        return false;
    }
    let branch = git2::Branch::wrap(head);
    let Ok(upstream) = branch.upstream() else {
        return false;
    };
    let Some(up_oid) = upstream.get().target() else {
        return false;
    };
    up_oid == commit || repo.graph_descendant_of(up_oid, commit).unwrap_or(false)
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

    /// Writes go to a throwaway config file — a test must never touch the real
    /// `~/.gitconfig` of whoever is running it.
    #[test]
    fn identity_round_trips_through_a_config_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("gitconfig");
        let mut cfg = git2::Config::open(&path).unwrap();

        write_identity(&mut cfg, "Ada Lovelace", "ada@example.com").unwrap();

        // Re-open from disk: proves it persisted rather than just cached.
        let reread = git2::Config::open(&path).unwrap();
        assert_eq!(reread.get_string("user.name").unwrap(), "Ada Lovelace");
        assert_eq!(reread.get_string("user.email").unwrap(), "ada@example.com");
    }

    #[test]
    fn saving_identity_creates_a_config_file_that_did_not_exist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("brand-new-gitconfig");
        assert!(!path.exists());

        let mut cfg = git2::Config::open(&path).unwrap();
        write_identity(&mut cfg, "Ada", "ada@example.com").unwrap();

        assert!(path.exists(), "first save must create the config file");
    }

    #[test]
    fn identity_is_trimmed_before_saving() {
        let (n, e) = validate_identity("  Ada Lovelace  ", "  ada@example.com  ").unwrap();
        assert_eq!(n, "Ada Lovelace");
        assert_eq!(e, "ada@example.com");
    }

    #[test]
    fn blank_name_is_rejected_with_a_specific_error() {
        let err = validate_identity("   ", "ada@example.com").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::NoIdentity));
        assert!(err.message.contains("name"));
    }

    #[test]
    fn email_without_a_usable_at_sign_is_rejected() {
        for bad in ["", "ada", "ada@", "@example.com", "   "] {
            let err = validate_identity("Ada", bad).unwrap_err();
            assert!(
                matches!(err.kind, ErrorKind::NoIdentity),
                "accepted bad email: {bad:?}"
            );
        }
    }

    /// GitHub's noreply form is what the dialog prefills, so it must validate.
    #[test]
    fn github_noreply_address_is_accepted() {
        assert!(validate_identity("sadlowskik", "sadlowskik@users.noreply.github.com").is_ok());
    }

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
    fn discard_restores_a_modified_file_to_head() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("f.txt"), "original\n").unwrap();
        stage_paths(&root, &[abs(&root, "f.txt")]).unwrap();
        commit(&root, "add f").unwrap();

        // Edit and stage the edit, then discard.
        fs::write(root.join("f.txt"), "ruined\n").unwrap();
        stage_paths(&root, &[abs(&root, "f.txt")]).unwrap();
        discard_paths(&root, &[abs(&root, "f.txt")]).unwrap();

        // Normalize CRLF: git may re-materialize with the platform line ending.
        let restored = fs::read_to_string(root.join("f.txt"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(restored, "original\n");
    }

    #[test]
    fn discard_restores_a_deleted_file() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("keep.txt"), "keep\n").unwrap();
        stage_paths(&root, &[abs(&root, "keep.txt")]).unwrap();
        commit(&root, "add").unwrap();

        fs::remove_file(root.join("keep.txt")).unwrap();
        discard_paths(&root, &[abs(&root, "keep.txt")]).unwrap();

        assert!(
            root.join("keep.txt").exists(),
            "deleted file should be restored"
        );
    }

    #[test]
    fn discard_leaves_untracked_files_alone() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("committed.txt"), "x\n").unwrap();
        stage_paths(&root, &[abs(&root, "committed.txt")]).unwrap();
        commit(&root, "c").unwrap();

        fs::write(root.join("scratch.txt"), "my notes\n").unwrap();
        // Discarding the whole repo dir must not delete untracked scratch.txt.
        discard_paths(&root, &[abs(&root, "scratch.txt")]).unwrap();

        assert!(
            root.join("scratch.txt").exists(),
            "untracked file must survive"
        );
    }

    #[test]
    fn amend_replaces_message_without_new_commit_parent() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("f.txt"), "a\n").unwrap();
        stage_paths(&root, &[abs(&root, "f.txt")]).unwrap();
        commit(&root, "typo mesage").unwrap();

        amend_commit(&root, "fixed message").unwrap();

        let repo = Repository::open(&root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "fixed message");
        assert_eq!(head.parent_count(), 0, "amend must not add a parent");
    }

    #[test]
    fn amend_with_blank_message_keeps_the_original() {
        let (_d, root) = repo_with_identity();
        fs::write(root.join("f.txt"), "a\n").unwrap();
        stage_paths(&root, &[abs(&root, "f.txt")]).unwrap();
        commit(&root, "keep me").unwrap();

        // New staged content, empty message → message preserved, content updated.
        fs::write(root.join("f.txt"), "b\n").unwrap();
        stage_paths(&root, &[abs(&root, "f.txt")]).unwrap();
        amend_commit(&root, "   ").unwrap();

        let repo = Repository::open(&root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "keep me");
        let blob = head.tree().unwrap().get_name("f.txt").unwrap().id();
        assert_eq!(
            std::str::from_utf8(repo.find_blob(blob).unwrap().content()).unwrap(),
            "b\n"
        );
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

/// Exercises the real `set_git_identity` — including `global_config()`, which the
/// other identity tests bypass — against a redirected global config location.
///
/// This is the case that actually breaks first-time users: a machine with no
/// `~/.gitconfig` at all, where `Config::find_global()` fails outright. Writing
/// has to *create* the file, and nothing short of driving the real function
/// proves that.
///
/// Lives in its own module because `set_search_path` mutates process-global
/// libgit2 state. It's safe here: every other test sets a *local* identity via
/// `repo_with_identity`, so none of them read the global level, and pointing it
/// at a temp dir only makes the suite more hermetic. A future test that relies
/// on the developer's real global config would be order-dependent — set a local
/// identity instead.
#[cfg(test)]
mod global_identity_tests {
    #[test]
    fn saves_identity_when_the_machine_has_no_gitconfig() {
        let dir = tempfile::TempDir::new().unwrap();
        // SAFETY: process-global; see the module comment for why that's OK here.
        unsafe {
            git2::opts::set_search_path(git2::ConfigLevel::Global, dir.path()).unwrap();
        }
        let expected = dir.path().join(".gitconfig");
        assert!(!expected.exists(), "precondition: no global config yet");

        super::set_git_identity("Ada Lovelace", "ada@example.com").unwrap();

        assert!(expected.exists(), "saving must create ~/.gitconfig");
        let written = std::fs::read_to_string(&expected).unwrap();
        assert!(written.contains("name = Ada Lovelace"), "got: {written}");
        assert!(
            written.contains("email = ada@example.com"),
            "got: {written}"
        );

        // And the app reads back what it just wrote — this is what unblocks the
        // commit that triggered the prompt.
        let id = super::git_identity();
        assert_eq!(id.name.as_deref(), Some("Ada Lovelace"));
        assert_eq!(id.email.as_deref(), Some("ada@example.com"));
    }
}
