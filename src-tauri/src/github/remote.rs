use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use git2::{
    build::RepoBuilder, FetchOptions, IndexAddOption, PushOptions, RemoteCallbacks, Repository,
    Signature,
};
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::git::ops::{open_repo, set_tracking};

use super::url::{is_github_token_url, parse_owner_repo, token_credentials};
use super::{api, auth};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub html_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrResult {
    pub html_url: String,
    pub number: u64,
}

fn current_branch(repo: &git2::Repository) -> AppResult<String> {
    let head = repo.head().map_err(|_| {
        AppError::new(
            ErrorKind::Git,
            "Make your first save (commit) before publishing.",
        )
    })?;
    if !head.is_branch() {
        return Err(AppError::new(
            ErrorKind::Git,
            "You’re not on a branch right now.",
        ));
    }
    Ok(head.shorthand().unwrap_or("main").to_string())
}

fn origin_owner_repo(repo_path: &Path) -> AppResult<(String, String)> {
    let repo = open_repo(repo_path)?;
    let remote = repo.find_remote("origin").map_err(|_| {
        AppError::new(
            ErrorKind::NoUpstream,
            "This folder isn’t connected to a GitHub repository yet.",
        )
    })?;
    let url = remote.url().unwrap_or_default().to_string();
    parse_owner_repo(&url).ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            "This repository’s remote isn’t a GitHub URL.",
        )
        .with_detail(url)
    })
}

fn token_callbacks(token: String) -> RemoteCallbacks<'static> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(token_credentials(token));
    cb
}

/// Set `origin` to the new URL and push the current branch, establishing
/// upstream tracking. Synchronous git2 work; call from `spawn_blocking`.
fn set_origin_and_push(
    repo_path: &Path,
    remote_url: &str,
    branch: &str,
    token: &str,
) -> AppResult<()> {
    let repo = open_repo(repo_path)?;
    if repo.find_remote("origin").is_ok() {
        repo.remote_set_url("origin", remote_url)?;
    } else {
        repo.remote("origin", remote_url)?;
    }

    let mut remote = repo.find_remote("origin")?;

    // The remote reports per-ref rejections through this callback rather than as
    // a returned Err. Without capturing it, a rejected push looks like success
    // and leaves an empty repo on GitHub.
    let rejection: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let rejection_cb = rejection.clone();

    let mut callbacks = token_callbacks(token.to_string());
    callbacks.push_update_reference(move |_refname, status| {
        if let Some(msg) = status {
            *rejection_cb.borrow_mut() = Some(msg.to_string());
        }
        Ok(())
    });

    let mut opts = PushOptions::new();
    opts.remote_callbacks(callbacks);
    let refspec = format!("refs/heads/{b}:refs/heads/{b}", b = branch);
    // Don't let a push failure surface as the generic "couldn't read Git data".
    remote.push(&[&refspec], Some(&mut opts)).map_err(|e| {
        let kind = if e.code() == git2::ErrorCode::Auth {
            ErrorKind::Auth
        } else {
            ErrorKind::Git
        };
        AppError::new(
            kind,
            "GitGlass created the repository on GitHub but couldn’t upload your files.",
        )
        .with_detail(e.to_string())
    })?;

    if let Some(msg) = rejection.borrow().clone() {
        return Err(crate::git::sync::push_rejection_error(&msg));
    }

    set_tracking(&repo, "origin", branch, "GitGlass: publish");
    Ok(())
}

/// Point `origin` at an existing GitHub repo, creating nothing on GitHub. Unlike
/// `publish` (which creates a new repo), this links a local folder to a repo that
/// already exists. The URL is validated as GitHub so `origin` never gets wired to
/// junk — and so the stored token is only ever offered to github.com.
///
/// After wiring the URL it fetches and sets up branch tracking (best-effort), so
/// "ahead / behind" works right away rather than staying blank until the first
/// push. An offline connect still succeeds; tracking is established later.
pub async fn set_origin(repo_path: &Path, url: &str) -> AppResult<()> {
    let url = url.trim().to_string();
    let repo_path = repo_path.to_path_buf();
    run_blocking(move || {
        set_origin_url(&repo_path, &url)?;
        // Best-effort — must not fail the connect if the remote is unreachable.
        let _ = crate::git::sync::fetch_and_track_origin(&repo_path);
        Ok(())
    })
    .await
}

/// Validate the URL as GitHub and set/replace `origin`. Split out so the
/// validation + wiring is unit-testable without the async fetch.
fn set_origin_url(repo_path: &Path, url: &str) -> AppResult<()> {
    let url = url.trim();
    parse_owner_repo(url).ok_or_else(|| {
        AppError::new(
            ErrorKind::NotFound,
            "That doesn’t look like a GitHub repository URL (e.g. https://github.com/owner/repo).",
        )
    })?;
    let repo = open_repo(repo_path)?;
    if repo.find_remote("origin").is_ok() {
        repo.remote_set_url("origin", url)?;
    } else {
        repo.remote("origin", url)?;
    }
    Ok(())
}

/// Ensure `folder` is a git repository with at least one commit, initializing it
/// and creating an initial commit if needed. Returns the branch to publish.
///
/// The initial commit is secret-scanned exactly like a normal commit — a plain
/// folder full of keys can't be silently pushed to GitHub. `fallback_*` provide
/// a committer identity (from the GitHub account) when git isn't configured
/// locally, so first-time users don't hit a wall.
fn ensure_repo_with_commit(
    folder: &Path,
    fallback_name: &str,
    fallback_email: &str,
    allow_secrets: bool,
) -> AppResult<String> {
    // If we create .git here and then fail, roll it back so a failed publish
    // never leaves a half-made repo behind.
    let created_here = Repository::discover(folder).is_err();
    let result = ensure_repo_inner(folder, fallback_name, fallback_email, allow_secrets);
    if result.is_err() && created_here {
        // `ensure_repo_inner` has returned, so its Repository handle is dropped
        // and Windows has released the files.
        let _ = std::fs::remove_dir_all(folder.join(".git"));
    }
    result
}

fn ensure_repo_inner(
    folder: &Path,
    fallback_name: &str,
    fallback_email: &str,
    allow_secrets: bool,
) -> AppResult<String> {
    // Git cannot track a repository inside a repository — catch it up front
    // with a message that says exactly which folder is the problem.
    let nested = crate::git::ops::find_embedded_repos(folder);
    if !nested.is_empty() {
        let list = nested.join(", ");
        return Err(AppError::new(
            ErrorKind::Git,
            format!(
                "“{list}” is its own Git repository inside this folder. Git can’t put a repository inside another one — remove that folder’s .git (or move it out), then try again."
            ),
        )
        .with_detail(format!("Embedded repositories: {list}")));
    }

    let repo = match Repository::discover(folder) {
        Ok(r) => r,
        Err(_) => Repository::init(folder)?,
    };

    // Already has history — just report the current branch.
    if repo.head().is_ok() {
        // The staged-diff gate further down only ever runs on an unborn branch,
        // so without this a folder that already had commits was published with
        // no scan at all — while the dialog said publishing was blocked until
        // secrets were removed. Scan the working tree here: `scan_staged` would
        // see an empty diff and wave it through.
        //
        // Limitation, deliberately not papered over: this covers the working
        // tree, not history. A secret that was committed and later deleted is
        // still in the objects being pushed and will not be caught here.
        if !allow_secrets {
            let findings = crate::secret_scan::scan_folder(folder)?;
            if !findings.is_empty() {
                return Err(AppError::new(
                    ErrorKind::SecretsFound,
                    "This folder looks like it contains secrets. Review and remove them (or add a .gitignore) before publishing.",
                )
                .with_detail(format!("{} suspected secret(s) found.", findings.len())));
            }
        }
        return current_branch(&repo);
    }

    // Unborn branch: build the first commit on `main`.
    let _ = repo.set_head("refs/heads/main");

    let mut index = repo.index()?;
    // Stage everything not covered by .gitignore (like `git add -A`).
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;

    // Secret gate on the initial import — same guarantee as committing. Skipped
    // only when the user completes the typed override in the publish dialog.
    if !allow_secrets {
        let findings = crate::secret_scan::scan_staged(folder)?;
        if !findings.is_empty() {
            return Err(AppError::new(
                ErrorKind::SecretsFound,
                "This folder looks like it contains secrets. Review and remove them (or add a .gitignore) before publishing.",
            )
            .with_detail(format!("{} suspected secret(s) found.", findings.len())));
        }
    }

    let signature = repo
        .signature()
        .or_else(|_| Signature::now(fallback_name, fallback_email))?;
    let tree = repo.find_tree(index.write_tree()?)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )?;

    Ok("main".to_string())
}

/// Create a GitHub repo from ANY local folder and push to it — initializing git
/// and making the first commit if the folder isn't a repo yet.
pub async fn publish(
    folder: &Path,
    name: &str,
    private: bool,
    description: Option<&str>,
    allow_secrets: bool,
) -> AppResult<PublishResult> {
    let token = auth::require_token()?;

    // A committer identity from the GitHub account, used only if git has none
    // configured locally (keeps first-run friction to zero).
    let user = api::get_user(&token).await.ok();
    let fb_name = user
        .as_ref()
        .map(|u| u.login.clone())
        .unwrap_or_else(|| "GitGlass User".to_string());
    let fb_email = user
        .as_ref()
        .map(|u| format!("{}@users.noreply.github.com", u.login))
        .unwrap_or_else(|| "gitglass@users.noreply.github.com".to_string());

    // Init + first commit (if needed) off the async runtime.
    let init_path = folder.to_path_buf();
    let branch = run_blocking(move || {
        ensure_repo_with_commit(&init_path, &fb_name, &fb_email, allow_secrets)
    })
    .await?;

    let created = api::create_repo(&token, name, private, description).await?;

    let path = folder.to_path_buf();
    let url = created.clone_url.clone();
    let br = branch.clone();
    let tok = token.clone();
    run_blocking(move || set_origin_and_push(&path, &url, &br, &tok)).await?;

    Ok(PublishResult {
        html_url: created.html_url,
    })
}

/// Clone a repository by URL into `dest`. Uses the stored token when present so
/// private repos work; public clones succeed without one.
///
/// The URL comes straight from the user (paste box), so it is not trusted: the
/// token is offered only when the host really is GitHub. Cloning elsewhere is
/// still allowed — it just proceeds unauthenticated rather than handing the
/// user's `repo`-scoped token to whoever owns that domain.
pub async fn clone(url: &str, dest: &Path) -> AppResult<PathBuf> {
    let token = auth::read_token().filter(|_| is_github_token_url(url));
    let url = url.to_string();
    let dest = dest.to_path_buf();
    run_blocking(move || {
        let mut cbs = RemoteCallbacks::new();
        if let Some(t) = token {
            // Re-checks the host on each callback, so a github.com URL that
            // redirects off-site still can't collect the token.
            cbs.credentials(token_credentials(t));
        }
        let mut fo = FetchOptions::new();
        fo.remote_callbacks(cbs);
        let mut builder = RepoBuilder::new();
        builder.fetch_options(fo);
        let repo = builder.clone(&url, &dest)?;
        Ok(repo.workdir().unwrap_or(&dest).to_path_buf())
    })
    .await
}

/// Open a pull request from the current branch into the repo's default branch.
pub async fn open_pr(repo_path: &Path, title: &str, body: &str) -> AppResult<PrResult> {
    let token = auth::require_token()?;
    let (owner, repo_name) = origin_owner_repo(repo_path)?;

    let branch = {
        let repo = open_repo(repo_path)?;
        current_branch(&repo)?
    };

    let gh_repo = api::get_repo(&token, &owner, &repo_name).await?;
    let base = gh_repo.default_branch.unwrap_or_else(|| "main".to_string());
    if base == branch {
        return Err(AppError::new(
            ErrorKind::Git,
            "You’re on the default branch. Create a branch with your changes first, then open a pull request.",
        ));
    }

    let created = api::create_pull(&token, &owner, &repo_name, title, &branch, &base, body).await?;
    Ok(PrResult {
        html_url: created.html_url,
        number: created.number,
    })
}

pub async fn list_prs(repo_path: &Path) -> AppResult<Vec<api::GhItem>> {
    let token = auth::require_token()?;
    let (owner, repo) = origin_owner_repo(repo_path)?;
    api::list_pulls(&token, &owner, &repo).await
}

pub async fn list_issues(repo_path: &Path) -> AppResult<Vec<api::GhItem>> {
    let token = auth::require_token()?;
    let (owner, repo) = origin_owner_repo(repo_path)?;
    api::list_issues(&token, &owner, &repo).await
}

/// All of the signed-in user's own repositories.
pub async fn list_my_repos() -> AppResult<Vec<api::RepoSummary>> {
    let token = auth::require_token()?;
    api::list_my_repos(&token).await
}

/// Recent Actions runs (automation status) for the current repo.
pub async fn list_runs(repo_path: &Path) -> AppResult<Vec<api::WorkflowRun>> {
    let token = auth::require_token()?;
    let (owner, repo) = origin_owner_repo(repo_path)?;
    api::list_workflow_runs(&token, &owner, &repo).await
}

/// Every automation (workflow) across the user's repos.
pub async fn list_automations() -> AppResult<Vec<api::Automation>> {
    let token = auth::require_token()?;
    api::list_automations(&token).await
}

/// Trigger a workflow run on demand ("Run now").
pub async fn run_now(repo_full_name: &str, workflow_id: u64, git_ref: &str) -> AppResult<()> {
    let token = auth::require_token()?;
    let mut parts = repo_full_name.splitn(2, '/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    api::run_workflow(&token, owner, repo, workflow_id, git_ref).await
}

/// Tag the current repo's default branch to cut a release. Pushing the tag
/// triggers the release workflow (Windows + macOS installers). Returns the
/// repo's Actions URL so the UI can send the user to watch the build.
pub async fn create_release(repo_path: &Path, tag: &str) -> AppResult<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(AppError::new(
            ErrorKind::Git,
            "Please enter a version, e.g. v1.0.0.",
        ));
    }
    let token = auth::require_token()?;
    let (owner, repo) = origin_owner_repo(repo_path)?;

    let gh_repo = api::get_repo(&token, &owner, &repo).await?;
    let branch = gh_repo.default_branch.unwrap_or_else(|| "main".to_string());
    let sha = api::branch_sha(&token, &owner, &repo, &branch).await?;
    api::create_tag(&token, &owner, &repo, tag, &sha).await?;

    Ok(format!("https://github.com/{owner}/{repo}/actions"))
}

/// Run blocking git2 work off the async runtime's worker threads.
async fn run_blocking<T, F>(f: F) -> AppResult<T>
where
    F: FnOnce() -> AppResult<T> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| {
        AppError::new(ErrorKind::Unknown, "A background task failed.").with_detail(e.to_string())
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn set_origin_accepts_a_github_url_and_wires_origin() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();

        set_origin_url(dir.path(), "https://github.com/owner/repo.git").unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        let origin = repo.find_remote("origin").unwrap();
        assert_eq!(origin.url(), Some("https://github.com/owner/repo.git"));
    }

    #[test]
    fn set_origin_rejects_a_non_github_url() {
        let dir = TempDir::new().unwrap();
        Repository::init(dir.path()).unwrap();

        let err = set_origin_url(dir.path(), "https://evil.example/owner/repo.git").unwrap_err();
        assert!(matches!(err.kind, ErrorKind::NotFound));
        // Nothing should have been wired up.
        let repo = Repository::open(dir.path()).unwrap();
        assert!(repo.find_remote("origin").is_err());
    }

    #[test]
    fn set_origin_updates_an_existing_origin() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.remote("origin", "https://github.com/old/old.git")
            .unwrap();

        set_origin_url(dir.path(), "https://github.com/new/new.git").unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        assert_eq!(
            repo.find_remote("origin").unwrap().url(),
            Some("https://github.com/new/new.git")
        );
    }

    #[test]
    fn ensure_repo_initializes_a_plain_folder() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.txt"), "hello world").unwrap();

        let branch = ensure_repo_with_commit(dir.path(), "Tester", "t@example.com", false).unwrap();
        assert_eq!(branch, "main");

        // The folder is now a repo whose HEAD is the initial commit.
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "Initial commit");
        assert!(head.tree().unwrap().get_name("readme.txt").is_some());
    }

    #[test]
    fn ensure_repo_reports_embedded_repo_clearly_and_rolls_back() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.txt"), "hi").unwrap();
        // A nested repository inside the folder we're publishing.
        let nested = dir.path().join("examples").join("demo");
        fs::create_dir_all(&nested).unwrap();
        Repository::init(&nested).unwrap();

        let err = ensure_repo_with_commit(dir.path(), "T", "t@e.com", false).unwrap_err();
        // The message must name the offending folder, not say "couldn't read Git data".
        assert!(
            err.message.contains("examples/demo"),
            "expected the nested repo to be named, got: {}",
            err.message
        );
        // And nothing half-made is left behind at the root.
        assert!(
            !dir.path().join(".git").exists(),
            "a failed publish must roll back the .git it created"
        );
    }

    #[test]
    fn ensure_repo_blocks_secrets_on_initial_import() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("config.txt"),
            "aws_key=AKIA1234567890ABCDEF",
        )
        .unwrap();

        let err = ensure_repo_with_commit(dir.path(), "T", "t@e.com", false).unwrap_err();
        assert!(matches!(err.kind, crate::error::ErrorKind::SecretsFound));

        // With the override, the same folder publishes past the gate.
        let ok = ensure_repo_with_commit(dir.path(), "T", "t@e.com", true);
        assert!(ok.is_ok(), "override should skip the secret gate");
    }

    /// The gate above only ever ran on an unborn branch. A folder that already
    /// had commits returned early, so publishing an existing repo — history and
    /// all — did no secret scanning whatsoever, while the dialog told the user
    /// publishing was blocked until secrets were removed.
    #[test]
    fn ensure_repo_blocks_secrets_when_the_repo_already_has_commits() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();

        // A real first commit, so `repo.head()` succeeds and we take the early
        // return path.
        fs::write(dir.path().join("readme.md"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("readme.md")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@e.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(index);
        drop(tree);

        // Now drop a secret into the working tree, uncommitted and unstaged —
        // exactly what `scan_staged` cannot see.
        fs::write(
            dir.path().join("config.txt"),
            "aws_key=AKIA1234567890ABCDEF",
        )
        .unwrap();

        let err = ensure_repo_with_commit(dir.path(), "T", "t@e.com", false).unwrap_err();
        assert!(
            matches!(err.kind, crate::error::ErrorKind::SecretsFound),
            "expected SecretsFound, got {:?}",
            err.kind
        );

        // And the typed override still gets through.
        assert!(ensure_repo_with_commit(dir.path(), "T", "t@e.com", true).is_ok());
    }

    /// Guard against the fix over-blocking: a clean existing repo must publish.
    #[test]
    fn ensure_repo_allows_a_clean_repo_that_already_has_commits() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        fs::write(dir.path().join("main.py"), "print('hello')\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("main.py")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@e.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        drop(index);
        drop(tree);

        assert!(ensure_repo_with_commit(dir.path(), "T", "t@e.com", false).is_ok());
    }

    #[test]
    fn parses_https_url() {
        assert_eq!(
            parse_owner_repo("https://github.com/acme/widgets.git"),
            Some(("acme".into(), "widgets".into()))
        );
    }

    #[test]
    fn parses_https_without_git_suffix() {
        assert_eq!(
            parse_owner_repo("https://github.com/acme/widgets"),
            Some(("acme".into(), "widgets".into()))
        );
    }

    #[test]
    fn parses_ssh_url() {
        assert_eq!(
            parse_owner_repo("git@github.com:acme/widgets.git"),
            Some(("acme".into(), "widgets".into()))
        );
    }

    #[test]
    fn rejects_non_github_url() {
        assert_eq!(
            parse_owner_repo("https://gitlab.com/acme/widgets.git"),
            None
        );
    }
}
