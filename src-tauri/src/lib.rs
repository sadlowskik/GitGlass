mod automation;
mod error;
mod fs;
mod git;
mod github;
mod secret_scan;
mod watcher;

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use error::AppResult;
use fs::DirListingDto;
use watcher::WatchState;

/// List a directory with inline git status. Re-arms the file watcher on the
/// repo the listed path belongs to, so the UI receives live "fs:changed"
/// events for that repo.
#[tauri::command]
fn list_dir(
    app: AppHandle,
    watch: State<'_, WatchState>,
    path: String,
) -> AppResult<DirListingDto> {
    let path = PathBuf::from(path);
    let listing = fs::list_dir(&path)?;

    if let Some(repo) = &listing.repo {
        watch.watch(&app, PathBuf::from(&repo.root));
    }
    Ok(listing)
}

#[tauri::command]
fn home_dir() -> AppResult<String> {
    Ok(fs::home_dir()?.to_string_lossy().to_string())
}

/// Reveal a path in the OS file manager. Best-effort; failure is non-fatal.
#[tauri::command]
fn reveal_in_os(path: String) -> AppResult<()> {
    // Uses the OS "opener" conventions; wired to a plugin in a later milestone.
    // For M1 we simply validate the path exists so the UI can trust the result.
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(error::AppError::new(
            error::ErrorKind::NotFound,
            "That item no longer exists.",
        ));
    }
    Ok(())
}

// --- M2: Git actions --------------------------------------------------------
// All operations take the repo root (the frontend already knows it from the
// listing) plus absolute paths, and go through git2 — never the git CLI.

#[tauri::command]
fn stage_paths(repo: String, paths: Vec<String>) -> AppResult<()> {
    git::ops::stage_paths(&PathBuf::from(repo), &paths)
}

#[tauri::command]
fn unstage_paths(repo: String, paths: Vec<String>) -> AppResult<()> {
    git::ops::unstage_paths(&PathBuf::from(repo), &paths)
}

/// Scan the staged changes for secrets. The UI calls this before committing to
/// show a rich block dialog; the commit command re-scans as enforcement.
#[tauri::command]
fn scan_staged(repo: String) -> AppResult<Vec<secret_scan::Finding>> {
    secret_scan::scan_staged(&PathBuf::from(repo))
}

/// Dry-run scan of every file a folder would publish — lets the user preview
/// secret leaks *before* publishing. Works on plain folders, not just repos.
#[tauri::command]
fn scan_folder(path: String) -> AppResult<Vec<secret_scan::Finding>> {
    secret_scan::scan_folder(&PathBuf::from(path))
}

/// Commit the staged index. Returns the new commit's oid string.
///
/// Secret gate: unless `allow_secrets` is true, the staged changes are scanned
/// and the commit is BLOCKED if anything is found. `allow_secrets` is only ever
/// set after the user completes the typed-confirmation override in the UI —
/// there is no one-click bypass, and this check runs even if a caller skipped
/// the pre-scan.
#[tauri::command]
fn commit(repo: String, message: String, allow_secrets: bool) -> AppResult<String> {
    let repo_path = PathBuf::from(repo);
    if !allow_secrets {
        let findings = secret_scan::scan_staged(&repo_path)?;
        if !findings.is_empty() {
            return Err(error::AppError::new(
                error::ErrorKind::SecretsFound,
                "Blocked: these changes look like they contain secrets.",
            )
            .with_detail(format!("{} suspected secret(s) found.", findings.len())));
        }
    }
    git::ops::commit(&repo_path, &message)
}

/// Append paths to .gitignore (creating it if needed) and unstage them. Used by
/// the secret dialog's "add to .gitignore" action.
#[tauri::command]
fn ignore_paths(repo: String, paths: Vec<String>) -> AppResult<()> {
    git::ops::ignore_paths(&PathBuf::from(repo), &paths)
}

/// The user's configured Git identity (may be empty for first-time users).
#[tauri::command]
fn git_identity() -> AppResult<git::ops::Identity> {
    Ok(git::ops::git_identity())
}

/// Turn a plain folder into a local Git repository with a first commit.
/// Entirely offline — no GitHub account, nothing uploaded.
#[tauri::command]
fn init_repo(
    path: String,
    name: Option<String>,
    email: Option<String>,
    allow_secrets: bool,
) -> AppResult<()> {
    git::ops::init_repo(
        &PathBuf::from(path),
        name.as_deref(),
        email.as_deref(),
        allow_secrets,
    )
}

/// Fetch + fast-forward the current branch. Returns a friendly summary.
#[tauri::command]
fn pull(repo: String) -> AppResult<String> {
    git::sync::pull(&PathBuf::from(repo))
}

/// Push the current branch to its upstream. Returns a friendly summary.
#[tauri::command]
fn push(repo: String) -> AppResult<String> {
    git::sync::push(&PathBuf::from(repo))
}

// --- M5: Diff viewer & branches ---------------------------------------------

/// Structured diff of a single file (staged + unstaged combined vs HEAD).
#[tauri::command]
fn file_diff(repo: String, path: String) -> AppResult<git::diff::FileDiffDto> {
    git::diff::file_diff(&PathBuf::from(repo), &path)
}

#[tauri::command]
fn list_branches(repo: String) -> AppResult<Vec<git::branch::BranchInfoDto>> {
    git::branch::list_branches(&PathBuf::from(repo))
}

#[tauri::command]
fn create_branch(repo: String, name: String, checkout: bool) -> AppResult<()> {
    git::branch::create_branch(&PathBuf::from(repo), &name, checkout)
}

#[tauri::command]
fn switch_branch(repo: String, name: String) -> AppResult<()> {
    git::branch::switch_branch(&PathBuf::from(repo), &name)
}

#[tauri::command]
fn delete_branch(repo: String, name: String) -> AppResult<()> {
    git::branch::delete_branch(&PathBuf::from(repo), &name)
}

// --- Automations: scheduled Python GitHub Actions ---------------------------

#[tauri::command]
fn list_python_files(repo: String) -> AppResult<Vec<String>> {
    automation::list_python_files(&PathBuf::from(repo))
}

#[tauri::command]
fn list_workflows(repo: String) -> AppResult<Vec<automation::WorkflowInfo>> {
    automation::list_workflows(&PathBuf::from(repo))
}

#[tauri::command]
fn automation_context(repo: String) -> AppResult<bool> {
    automation::has_requirements(&PathBuf::from(repo))
}

/// Generate a scheduled-Python workflow file. Returns its absolute path so the
/// UI can point the user at it to stage/commit/push (which activates it).
#[tauri::command]
fn create_workflow(
    repo: String,
    name: String,
    python_path: String,
    cron: String,
    python_version: String,
    install_requirements: bool,
) -> AppResult<String> {
    automation::create_workflow(
        &PathBuf::from(repo),
        &name,
        &python_path,
        &cron,
        &python_version,
        install_requirements,
    )
}

// --- M4: GitHub -------------------------------------------------------------

#[tauri::command]
async fn github_status() -> AppResult<github::AuthStatus> {
    Ok(github::auth::current_status().await)
}

#[tauri::command]
async fn github_start_login(client_id: String) -> AppResult<github::DeviceCodeDto> {
    github::auth::start_login(&client_id).await
}

#[tauri::command]
async fn github_poll_login(client_id: String, device_code: String) -> AppResult<github::LoginPoll> {
    github::auth::poll_login(&client_id, &device_code).await
}

#[tauri::command]
fn github_sign_out() -> AppResult<()> {
    github::auth::sign_out()
}

/// Create a GitHub repo from a local folder and push to it.
#[tauri::command]
async fn github_publish(
    repo: String,
    name: String,
    private: bool,
    description: Option<String>,
    allow_secrets: bool,
) -> AppResult<github::PublishResult> {
    github::remote::publish(
        &PathBuf::from(repo),
        &name,
        private,
        description.as_deref(),
        allow_secrets,
    )
    .await
}

/// Clone a repository by URL into `dest`; resolves to the working-dir path.
#[tauri::command]
async fn github_clone(url: String, dest: String) -> AppResult<String> {
    let path = github::remote::clone(&url, &PathBuf::from(dest)).await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn github_open_pr(repo: String, title: String, body: String) -> AppResult<github::PrResult> {
    github::remote::open_pr(&PathBuf::from(repo), &title, &body).await
}

#[tauri::command]
async fn github_list_prs(repo: String) -> AppResult<Vec<github::api::GhItem>> {
    github::remote::list_prs(&PathBuf::from(repo)).await
}

#[tauri::command]
async fn github_list_issues(repo: String) -> AppResult<Vec<github::api::GhItem>> {
    github::remote::list_issues(&PathBuf::from(repo)).await
}

/// The signed-in user's own repositories (account-wide).
#[tauri::command]
async fn github_my_repos() -> AppResult<Vec<github::api::RepoSummary>> {
    github::remote::list_my_repos().await
}

/// Recent Actions runs for the current repo (automation status).
#[tauri::command]
async fn github_workflow_runs(repo: String) -> AppResult<Vec<github::api::WorkflowRun>> {
    github::remote::list_runs(&PathBuf::from(repo)).await
}

/// Every automation (workflow) across the user's repos — for the sidebar.
#[tauri::command]
async fn github_automations() -> AppResult<Vec<github::api::Automation>> {
    github::remote::list_automations().await
}

/// Trigger a workflow run on demand.
#[tauri::command]
async fn github_run_now(
    repo_full_name: String,
    workflow_id: u64,
    git_ref: String,
) -> AppResult<()> {
    github::remote::run_now(&repo_full_name, workflow_id, &git_ref).await
}

/// Cut a release by tagging the default branch (kicks off the build workflow).
/// Returns the repo's Actions URL to watch progress.
#[tauri::command]
async fn github_create_release(repo: String, tag: String) -> AppResult<String> {
    github::remote::create_release(&PathBuf::from(repo), &tag).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(WatchState::default())
        .setup(|app| {
            // Ensure the main window label matches the capability scope.
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_dir,
            home_dir,
            reveal_in_os,
            stage_paths,
            unstage_paths,
            commit,
            scan_staged,
            scan_folder,
            ignore_paths,
            git_identity,
            init_repo,
            pull,
            push,
            file_diff,
            list_branches,
            create_branch,
            switch_branch,
            delete_branch,
            list_python_files,
            list_workflows,
            automation_context,
            create_workflow,
            github_status,
            github_start_login,
            github_poll_login,
            github_sign_out,
            github_publish,
            github_clone,
            github_open_pr,
            github_list_prs,
            github_list_issues,
            github_my_repos,
            github_workflow_runs,
            github_automations,
            github_run_now,
            github_create_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitGlass");
}
