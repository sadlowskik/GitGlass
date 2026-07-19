use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use git2::{
    build::CheckoutBuilder, Cred, CredentialType, FetchOptions, PushOptions, RemoteCallbacks,
    Repository,
};

use crate::error::{AppError, AppResult, ErrorKind};

use super::ops::{open_repo, set_tracking};

/// Build credential callbacks. Order of preference:
///   1. the GitHub token from the OS keychain (when signed in, for github.com)
///   2. the user's Git credential helper (e.g. Git Credential Manager)
///   3. the SSH agent for ssh:// remotes
///
/// (1) matters: after signing in, push/pull must use that token — most users
/// have no credential helper configured at all.
fn credentials_cb(
    config: git2::Config,
) -> impl FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, git2::Error> {
    move |url, username_from_url, allowed| {
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            // Only real github.com over HTTPS may receive the stored token; a
            // look-alike host falls through to the credential helper, which
            // decides for itself what (if anything) that host is owed.
            if crate::github::url::is_github_token_url(url) {
                if let Some(token) = crate::github::auth::read_token() {
                    return Cred::userpass_plaintext("x-access-token", &token);
                }
            }
            return Cred::credential_helper(&config, url, username_from_url);
        }
        if allowed.contains(CredentialType::SSH_KEY) {
            return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
        }
        if allowed.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }
        Err(git2::Error::from_str(
            "no supported credential type available",
        ))
    }
}

/// Translate remote/transport errors into specific, friendly `AppError`s.
fn map_remote_error(e: git2::Error) -> AppError {
    use git2::ErrorCode;
    match e.code() {
        ErrorCode::Auth | ErrorCode::Certificate => AppError::new(
            ErrorKind::Auth,
            "GitGlass couldn’t sign in to the remote. If it’s on GitHub, sign in to GitHub in GitGlass; otherwise check the credentials your Git setup uses for it.",
        )
        .with_detail(e.to_string()),
        _ => AppError::new(
            ErrorKind::Git,
            "GitGlass couldn’t reach the remote repository.",
        )
        .with_detail(e.to_string()),
    }
}

/// Turn a per-ref push rejection from the remote into a friendly, actionable
/// error. Shared with the publish path so both explain the same failures.
pub(crate) fn push_rejection_error(msg: &str) -> AppError {
    // GitHub refuses any push touching .github/workflows unless the token
    // carries the `workflow` scope. Sign-in grants it, but a token minted
    // before we asked for that scope won't have it.
    if msg.contains("workflow") && msg.contains("scope") {
        return AppError::new(
            ErrorKind::Auth,
            "GitHub needs extra permission to upload automation files (.github/workflows). Sign out of GitHub in GitGlass and sign in again to grant it, then push.",
        )
        .with_detail(msg.to_string());
    }
    // Most common rejection is non-fast-forward ("fetch first").
    if msg.contains("fetch first") || msg.contains("non-fast-forward") {
        return AppError::new(
            ErrorKind::NonFastForward,
            "The remote has changes you don’t have yet. Pull first, then push.",
        )
        .with_detail(msg.to_string());
    }
    AppError::new(ErrorKind::Git, "The remote rejected the push.").with_detail(msg.to_string())
}

struct Upstream {
    remote: String,
    branch: String,
}

/// Resolve the current branch and its upstream remote, or a specific error.
fn resolve_upstream(repo: &Repository) -> AppResult<Upstream> {
    let head = repo
        .head()
        .map_err(|_| AppError::new(ErrorKind::NoUpstream, "You have no commits to sync yet."))?;
    if !head.is_branch() {
        return Err(AppError::new(
            ErrorKind::NoUpstream,
            "You’re not on a branch, so there’s nothing to sync.",
        ));
    }
    let refname = head.name().unwrap_or_default().to_string();
    let branch = head.shorthand().unwrap_or_default().to_string();

    // Prefer the configured upstream. If tracking was never set but an `origin`
    // exists (e.g. a repo that was just published), fall back to it — that's
    // what `git push -u origin <branch>` does, and it's what the user means.
    let remote = match repo.branch_upstream_remote(&refname) {
        Ok(r) => r.as_str().unwrap_or("origin").to_string(),
        Err(_) if repo.find_remote("origin").is_ok() => "origin".to_string(),
        Err(_) => {
            return Err(AppError::new(
                ErrorKind::NoUpstream,
                "This branch isn’t connected to a remote yet. Use Publish to put it on GitHub.",
            ))
        }
    };

    Ok(Upstream { remote, branch })
}

/// Fetch from the upstream remote and fast-forward the current branch. Diverged
/// histories (a real merge) are reported as a friendly error — merge support
/// arrives in a later milestone.
pub fn pull(repo_path: &Path) -> AppResult<String> {
    let repo = open_repo(repo_path)?;
    let up = resolve_upstream(&repo)?;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials_cb(repo.config()?));
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    // Bring tags down too, so release/version tags show up locally.
    fetch_opts.download_tags(git2::AutotagOption::All);

    let mut remote = repo.find_remote(&up.remote).map_err(map_remote_error)?;
    remote
        .fetch(&[&up.branch], Some(&mut fetch_opts), None)
        .map_err(map_remote_error)?;

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

    if analysis.is_up_to_date() {
        return Ok("You’re already up to date.".into());
    }
    if !analysis.is_fast_forward() {
        return Err(AppError::new(
            ErrorKind::NonFastForward,
            "Your branch and the remote have both moved on, so combining them needs a merge — which GitGlass can’t do yet. To keep going now: create a branch to save your work, then pull; or merge with Git directly. (In-app merging is coming.)",
        ));
    }

    // Fast-forward. Update the working tree with a SAFE checkout so uncommitted
    // work is never silently overwritten — real `git pull` refuses a fast-forward
    // that would clobber local changes, and so must we.
    //
    // Order matters: checkout FIRST, while HEAD is still the old commit, so the
    // checkout's baseline is the old tree and it (a) actually applies the
    // incoming changes and (b) detects a conflicting local edit. Only once the
    // working tree is updated do we advance the branch ref. A SAFE checkout that
    // hits a conflict makes no changes and returns an error, so a refused pull is
    // a clean no-op with nothing to roll back.
    let target = repo.find_object(fetch_commit.id(), None)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&target, Some(&mut checkout))
        .map_err(|e| {
            AppError::new(
            ErrorKind::Git,
            "Pull would overwrite unsaved changes. Save (commit) or discard them first, then pull.",
        )
        .with_detail(e.to_string())
        })?;

    let refname = format!("refs/heads/{}", up.branch);
    let mut reference = repo.find_reference(&refname)?;
    reference.set_target(fetch_commit.id(), "GitGlass: fast-forward pull")?;
    repo.set_head(&refname)?;

    Ok("Pulled the latest changes.".into())
}

/// Push the current branch to its upstream remote. A non-fast-forward rejection
/// (remote is ahead) is surfaced as "pull first", not a raw git error.
pub fn push(repo_path: &Path) -> AppResult<String> {
    let repo = open_repo(repo_path)?;
    let up = resolve_upstream(&repo)?;

    // The remote reports per-ref rejections through this callback rather than as
    // a returned Err, so capture the rejection message here.
    let rejection: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let rejection_cb = rejection.clone();

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials_cb(repo.config()?));
    callbacks.push_update_reference(move |_refname, status| {
        if let Some(msg) = status {
            *rejection_cb.borrow_mut() = Some(msg.to_string());
        }
        Ok(())
    });

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    let mut remote = repo.find_remote(&up.remote).map_err(map_remote_error)?;
    let refspec = format!("refs/heads/{b}:refs/heads/{b}", b = up.branch);
    remote
        .push(&[&refspec], Some(&mut push_opts))
        .map_err(map_remote_error)?;

    if let Some(msg) = rejection.borrow().clone() {
        return Err(push_rejection_error(&msg));
    }

    set_tracking(&repo, &up.remote, &up.branch, "GitGlass: push");

    // Also publish local tags so releases/versions appear on GitHub. A tag that
    // diverged from the remote is rejected; that must NOT fail the (successful)
    // branch push, but we do surface it softly rather than swallowing it.
    let tag_note = match push_tags(&repo, &up.remote) {
        Ok(()) => "",
        Err(_) => " Some tags couldn’t be published (they differ from the remote).",
    };

    Ok(format!("Pushed your changes.{tag_note}"))
}

/// Push every local tag to `remote`. Best-effort and idempotent — tags already
/// on the remote are no-ops. Errors are the caller's to ignore.
fn push_tags(repo: &Repository, remote_name: &str) -> AppResult<()> {
    let tags = repo.tag_names(None)?;
    let refspecs: Vec<String> = tags
        .iter()
        .flatten()
        .map(|t| format!("refs/tags/{t}:refs/tags/{t}"))
        .collect();
    if refspecs.is_empty() {
        return Ok(());
    }
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials_cb(repo.config()?));
    let mut opts = PushOptions::new();
    opts.remote_callbacks(callbacks);
    let mut remote = repo.find_remote(remote_name)?;
    let refs: Vec<&str> = refspecs.iter().map(String::as_str).collect();
    remote.push(&refs, Some(&mut opts))?;
    Ok(())
}

/// Fetch from the remote without merging: refreshes remote-tracking refs and
/// tags so "ahead / behind" reflects reality. Never touches the working tree.
/// This is the only way to see you're behind without a (destructive-risk) pull.
pub fn fetch(repo_path: &Path) -> AppResult<String> {
    let repo = open_repo(repo_path)?;
    let remote_name = fetch_remote_name(&repo)?;
    fetch_from(&repo, &remote_name)?;
    Ok("Fetched the latest from the remote.".into())
}

/// Fetch a named remote's branches + tags, updating remote-tracking refs. Never
/// merges or touches the working tree.
fn fetch_from(repo: &Repository, remote_name: &str) -> AppResult<()> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(credentials_cb(repo.config()?));
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.download_tags(git2::AutotagOption::All);

    let mut remote = repo.find_remote(remote_name).map_err(map_remote_error)?;
    // Empty refspec list → use the remote's configured refspecs (all branches).
    remote
        .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
        .map_err(map_remote_error)?;
    Ok(())
}

/// Fetch from `origin` and set the current branch to track `origin/<branch>` if
/// that ref now exists. Used right after connecting a local repo to an existing
/// GitHub repo, so "ahead / behind" works immediately instead of staying blank
/// until the first push. Best-effort by the caller: an offline connect still
/// sets the URL, and a later push/pull establishes tracking.
pub fn fetch_and_track_origin(repo_path: &Path) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    // Fetch `origin` EXPLICITLY, not via fetch_remote_name's preference logic:
    // a branch that already tracks some *other* remote must not divert this, or
    // origin/<branch> wouldn't exist and tracking wouldn't get set.
    fetch_from(&repo, "origin")?;

    let head = repo.head()?;
    if !head.is_branch() {
        return Ok(());
    }
    if let Some(branch) = head.shorthand() {
        let tracking = format!("refs/remotes/origin/{branch}");
        if repo.find_reference(&tracking).is_ok() {
            if let Ok(mut local) = repo.find_branch(branch, git2::BranchType::Local) {
                let _ = local.set_upstream(Some(&format!("origin/{branch}")));
            }
        }
    }
    Ok(())
}

/// Pick a remote to fetch from: the current branch's upstream, else `origin`,
/// else the only remote configured.
fn fetch_remote_name(repo: &Repository) -> AppResult<String> {
    if let Ok(head) = repo.head() {
        if head.is_branch() {
            if let Ok(r) = repo.branch_upstream_remote(head.name().unwrap_or_default()) {
                if let Some(s) = r.as_str() {
                    return Ok(s.to_string());
                }
            }
        }
    }
    if repo.find_remote("origin").is_ok() {
        return Ok("origin".to_string());
    }
    let remotes = repo.remotes()?;
    if let Some(first) = remotes.iter().flatten().next() {
        return Ok(first.to_string());
    }
    Err(AppError::new(
        ErrorKind::NoUpstream,
        "This repository isn’t connected to a remote yet.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Build an origin repo and a working clone of it. Returns (origin_dir,
    /// origin_repo, clone_dir, clone_path). Local path remote → no credentials.
    fn origin_and_clone() -> (tempfile::TempDir, Repository, tempfile::TempDir, PathBuf) {
        let origin_dir = tempfile::tempdir().unwrap();
        let origin = Repository::init(origin_dir.path()).unwrap();
        fs::write(origin_dir.path().join("shared.txt"), "v1\n").unwrap();
        commit_all(&origin, "v1");

        let clone_dir = tempfile::tempdir().unwrap();
        let clone_path = clone_dir.path().join("wc");
        let url = url::Url::from_file_path(origin_dir.path()).unwrap();
        let clone = Repository::clone(url.as_str(), &clone_path).unwrap();
        let mut cfg = clone.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@e.com").unwrap();
        (origin_dir, origin, clone_dir, clone_path)
    }

    fn commit_all(repo: &Repository, msg: &str) -> git2::Oid {
        let mut idx = repo.index().unwrap();
        idx.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        idx.write().unwrap();
        let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@e.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents)
            .unwrap()
    }

    fn head_oid(path: &Path) -> git2::Oid {
        Repository::open(path)
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap()
    }

    #[test]
    fn clean_fast_forward_pull_succeeds() {
        let (origin_dir, origin, _cd, clone_path) = origin_and_clone();
        fs::write(origin_dir.path().join("shared.txt"), "v2\n").unwrap();
        let advanced = commit_all(&origin, "v2");

        pull(&clone_path).unwrap();

        assert_eq!(
            head_oid(&clone_path),
            advanced,
            "clone should be fast-forwarded"
        );
        assert_eq!(
            fs::read_to_string(clone_path.join("shared.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "v2\n"
        );
    }

    #[test]
    fn pull_refuses_to_overwrite_a_conflicting_local_edit() {
        let (origin_dir, origin, _cd, clone_path) = origin_and_clone();
        let before = head_oid(&clone_path);
        fs::write(origin_dir.path().join("shared.txt"), "v2\n").unwrap();
        commit_all(&origin, "v2");

        // Uncommitted local edit to the SAME file the pull would change.
        fs::write(clone_path.join("shared.txt"), "MY UNSAVED WORK\n").unwrap();

        let err = pull(&clone_path).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::Git));
        assert!(err.message.contains("unsaved"), "got: {}", err.message);

        // The edit survives, and the branch did NOT move — a refused pull is a no-op.
        assert_eq!(
            fs::read_to_string(clone_path.join("shared.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "MY UNSAVED WORK\n"
        );
        assert_eq!(
            head_oid(&clone_path),
            before,
            "branch must not advance on a refused pull"
        );
    }

    #[test]
    fn pull_preserves_an_unrelated_uncommitted_edit() {
        let (origin_dir, origin, _cd, clone_path) = origin_and_clone();
        // origin adds a NEW file; the local edit is to a different, untouched file.
        fs::write(origin_dir.path().join("added.txt"), "brand new\n").unwrap();
        let advanced = commit_all(&origin, "add file");

        fs::write(clone_path.join("shared.txt"), "my local tweak\n").unwrap();

        pull(&clone_path).unwrap();

        assert_eq!(head_oid(&clone_path), advanced, "should fast-forward");
        // The incoming file arrived AND the unrelated local edit is untouched.
        assert!(clone_path.join("added.txt").exists());
        assert_eq!(
            fs::read_to_string(clone_path.join("shared.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "my local tweak\n"
        );
    }

    #[test]
    fn fetch_updates_remote_tracking_and_downloads_tags() {
        let (origin_dir, origin, _cd, clone_path) = origin_and_clone();
        fs::write(origin_dir.path().join("shared.txt"), "v2\n").unwrap();
        let v2 = commit_all(&origin, "v2");
        origin
            .tag_lightweight("v1.0.0", &origin.find_object(v2, None).unwrap(), false)
            .unwrap();

        fetch(&clone_path).unwrap();

        let clone = Repository::open(&clone_path).unwrap();
        // Remote-tracking ref advanced (so ahead/behind will now be right)...
        let rt = clone
            .find_reference("refs/remotes/origin/master")
            .or_else(|_| clone.find_reference("refs/remotes/origin/main"))
            .unwrap();
        assert_eq!(
            rt.target().unwrap(),
            v2,
            "remote-tracking ref should advance"
        );
        // ...the tag came down...
        assert!(clone
            .tag_names(None)
            .unwrap()
            .iter()
            .flatten()
            .any(|t| t == "v1.0.0"));
        // ...and the working tree was NOT touched (fetch never merges).
        assert_eq!(
            fs::read_to_string(clone_path.join("shared.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "v1\n"
        );
    }

    #[test]
    fn connect_establishes_tracking_so_ahead_behind_works() {
        // Simulate "connect to an existing repo": a local repo with its own
        // commit, wired to an origin that already has history — no tracking yet.
        let origin_dir = tempfile::tempdir().unwrap();
        let origin = Repository::init(origin_dir.path()).unwrap();
        fs::write(origin_dir.path().join("remote.txt"), "remote\n").unwrap();
        commit_all(&origin, "remote commit");
        let branch = origin.head().unwrap().shorthand().unwrap().to_string();

        let local_dir = tempfile::tempdir().unwrap();
        let local = Repository::init(local_dir.path()).unwrap();
        // Match the origin's branch name so tracking can line up.
        local.set_head(&format!("refs/heads/{branch}")).unwrap();
        let mut cfg = local.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@e.com").unwrap();
        drop(cfg);
        fs::write(local_dir.path().join("local.txt"), "local\n").unwrap();
        commit_all(&local, "local commit");
        let url = url::Url::from_file_path(origin_dir.path()).unwrap();
        local.remote("origin", url.as_str()).unwrap();

        // Before: no upstream configured.
        assert!(local
            .find_branch(&branch, git2::BranchType::Local)
            .unwrap()
            .upstream()
            .is_err());

        fetch_and_track_origin(local_dir.path()).unwrap();

        // After: the branch tracks origin/<branch>, so ahead/behind is computable.
        let reopened = Repository::open(local_dir.path()).unwrap();
        let up = reopened
            .find_branch(&branch, git2::BranchType::Local)
            .unwrap()
            .upstream();
        assert!(up.is_ok(), "connect should establish upstream tracking");
    }

    #[test]
    fn push_tags_publishes_local_tags_to_the_remote() {
        let (_od, _origin, _cd, clone_path) = origin_and_clone();
        let clone = Repository::open(&clone_path).unwrap();
        // libgit2's local push only supports bare remotes, so target a bare repo
        // (a GitHub remote is effectively bare, so this mirrors production).
        let bare_dir = tempfile::tempdir().unwrap();
        let bare = Repository::init_bare(bare_dir.path()).unwrap();
        let url = url::Url::from_file_path(bare_dir.path()).unwrap();
        clone.remote("backup", url.as_str()).unwrap();

        let head = clone.head().unwrap().target().unwrap();
        clone
            .tag_lightweight("v2.0.0", &clone.find_object(head, None).unwrap(), false)
            .unwrap();

        push_tags(&clone, "backup").unwrap();

        assert!(bare
            .tag_names(None)
            .unwrap()
            .iter()
            .flatten()
            .any(|t| t == "v2.0.0"));
    }

    #[test]
    fn workflow_scope_rejection_is_actionable() {
        let msg = "refusing to allow an OAuth App to create or update workflow \
                   `.github/workflows/ci.yml` without `workflow` scope";
        let err = push_rejection_error(msg);
        assert!(matches!(err.kind, ErrorKind::Auth));
        // Must tell the user what to actually DO, not just restate git.
        assert!(err.message.contains("Sign out"), "got: {}", err.message);
    }

    #[test]
    fn non_fast_forward_rejection_says_pull_first() {
        let err = push_rejection_error("failed to push some refs (fetch first)");
        assert!(matches!(err.kind, ErrorKind::NonFastForward));
        assert!(err.message.contains("Pull first"));
    }

    #[test]
    fn unknown_rejection_keeps_raw_text_in_detail_only() {
        let err = push_rejection_error("some unexpected remote complaint");
        assert!(matches!(err.kind, ErrorKind::Git));
        // Friendly on the surface, raw text tucked into detail.
        assert!(!err.message.contains("unexpected remote complaint"));
        assert_eq!(
            err.detail.as_deref(),
            Some("some unexpected remote complaint")
        );
    }
}
