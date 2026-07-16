use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use git2::{
    build::CheckoutBuilder, Cred, CredentialType, FetchOptions, PushOptions, RemoteCallbacks,
    Repository,
};

use crate::error::{AppError, AppResult, ErrorKind};

use super::ops::open_repo;

/// Build credential callbacks. Until first-class GitHub OAuth lands (M4), we
/// bridge through the user's existing Git setup: SSH agent for SSH remotes and
/// the configured Git credential helper (e.g. Git Credential Manager) for HTTPS.
fn credentials_cb(
    config: git2::Config,
) -> impl FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, git2::Error> {
    move |url, username_from_url, allowed| {
        if allowed.contains(CredentialType::SSH_KEY) {
            return Cred::ssh_key_from_agent(username_from_url.unwrap_or("git"));
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::credential_helper(&config, url, username_from_url);
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
            "GitGlass couldn’t sign in to the remote. Full GitHub sign-in is coming soon; for now it uses your existing Git credentials.",
        )
        .with_detail(e.to_string()),
        _ => AppError::new(
            ErrorKind::Git,
            "GitGlass couldn’t reach the remote repository.",
        )
        .with_detail(e.to_string()),
    }
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

    let remote = repo.branch_upstream_remote(&refname).map_err(|_| {
        AppError::new(
            ErrorKind::NoUpstream,
            "This branch isn’t connected to a remote yet.",
        )
    })?;
    let remote = remote.as_str().unwrap_or("origin").to_string();

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
            "Your branch and the remote have both changed. Merging isn’t supported yet — this is coming soon.",
        ));
    }

    // Fast-forward: move the branch ref and update the working tree.
    let refname = format!("refs/heads/{}", up.branch);
    let mut reference = repo.find_reference(&refname)?;
    reference.set_target(fetch_commit.id(), "GitGlass: fast-forward pull")?;
    repo.set_head(&refname)?;
    repo.checkout_head(Some(CheckoutBuilder::default().force()))?;

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
        // Most common rejection is non-fast-forward ("fetch first").
        let non_ff = msg.contains("fetch first") || msg.contains("non-fast-forward");
        if non_ff {
            return Err(AppError::new(
                ErrorKind::NonFastForward,
                "The remote has changes you don’t have yet. Pull first, then push.",
            )
            .with_detail(msg));
        }
        return Err(
            AppError::new(ErrorKind::Git, "The remote rejected the push.").with_detail(msg),
        );
    }

    Ok("Pushed your changes.".into())
}
