use std::path::{Path, PathBuf};

use git2::{
    build::RepoBuilder, BranchType, Cred, FetchOptions, IndexAddOption, PushOptions,
    RemoteCallbacks, Repository, Signature,
};
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::git::ops::open_repo;

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

/// Parse `owner` and `repo` out of a GitHub remote URL (https or ssh forms).
pub fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = u
        .strip_prefix("https://github.com/")
        .or_else(|| u.strip_prefix("http://github.com/"))
        .or_else(|| u.strip_prefix("git@github.com:"))
        .or_else(|| u.find("github.com/").map(|i| &u[i + "github.com/".len()..]))?;
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.trim_end_matches('/').to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
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
    // GitHub accepts the token as the username with an empty password.
    cb.credentials(move |_url, _user, _allowed| Cred::userpass_plaintext(&token, ""));
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
    let mut opts = PushOptions::new();
    opts.remote_callbacks(token_callbacks(token.to_string()));
    let refspec = format!("refs/heads/{b}:refs/heads/{b}", b = branch);
    // Don't let a push failure surface as the generic "couldn't read Git data".
    remote.push(&[&refspec], Some(&mut opts)).map_err(|e| {
        AppError::new(
            ErrorKind::Git,
            "GitGlass created the repository on GitHub but couldn’t upload your files.",
        )
        .with_detail(e.to_string())
    })?;

    // Track origin/<branch> so sync state ("ahead/behind") works afterwards.
    let mut local = repo.find_branch(branch, BranchType::Local)?;
    local.set_upstream(Some(&format!("origin/{branch}")))?;
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
pub async fn clone(url: &str, dest: &Path) -> AppResult<PathBuf> {
    let token = auth::read_token();
    let url = url.to_string();
    let dest = dest.to_path_buf();
    run_blocking(move || {
        let mut cbs = RemoteCallbacks::new();
        if let Some(t) = token {
            cbs.credentials(move |_u, _user, _a| Cred::userpass_plaintext(&t, ""));
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
