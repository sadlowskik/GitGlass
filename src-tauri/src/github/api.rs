use std::sync::OnceLock;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};

const API: &str = "https://api.github.com";

/// Shared reqwest client. GitHub requires a User-Agent; we set a stable one.
pub fn http() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent("GitGlass")
            .build()
            .expect("failed to build http client")
    })
}

/// Attach the standard authenticated GitHub API headers.
fn authed(builder: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    builder
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
}

/// Turn a non-success GitHub response into a friendly error, mapping the common
/// status codes; the raw body goes into `detail` only.
async fn check(resp: reqwest::Response) -> AppResult<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let (kind, msg) = match status.as_u16() {
        401 => (
            ErrorKind::Auth,
            "Your GitHub sign-in has expired. Please sign in again.",
        ),
        403 => (
            ErrorKind::Auth,
            "GitHub declined the request (rate limit or permissions).",
        ),
        404 => (
            ErrorKind::NotFound,
            "That repository or resource wasn’t found on GitHub.",
        ),
        422 => (
            ErrorKind::Git,
            "GitHub rejected the request — the name may already be taken.",
        ),
        429 => (
            ErrorKind::Io,
            "GitHub is asking GitGlass to slow down. Wait a minute, then try again.",
        ),
        // GitHub's own outages and degraded spells land here. Nothing the user
        // did is wrong and nothing they change will help, so say that instead
        // of calling it "unexpected" and inviting them to hunt for a mistake.
        500..=599 => (
            ErrorKind::Io,
            "GitHub is having trouble on their end right now — nothing’s wrong with your repo. Check githubstatus.com, then try again in a few minutes.",
        ),
        _ => (ErrorKind::Io, "GitHub returned an unexpected error."),
    };
    Err(AppError::new(kind, msg).with_detail(describe_body(status, &body)))
}

/// Summarize a failed response for the details disclosure.
///
/// API errors come back as small JSON blobs worth showing verbatim, but during
/// an outage GitHub's edge serves a full HTML error page instead. Pasting that
/// page into the UI buries the status line in markup, so HTML is reduced to the
/// one fact it carries — and any body is capped, since `detail` is a disclosure
/// line, not a log viewer.
fn describe_body(status: reqwest::StatusCode, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return format!("HTTP {status} (empty response body)");
    }
    if body.starts_with('<') {
        return format!("HTTP {status} — GitHub returned an HTML error page, not an API response. This is a GitHub-side outage rather than a problem with the request.");
    }
    const MAX: usize = 500;
    if body.chars().count() > MAX {
        // Truncate by chars, not bytes: the body is UTF-8 and slicing mid-codepoint panics.
        let head: String = body.chars().take(MAX).collect();
        return format!("HTTP {status}: {head}…");
    }
    format!("HTTP {status}: {body}")
}

#[derive(Deserialize)]
pub struct GhUser {
    pub login: String,
    pub avatar_url: Option<String>,
}

pub async fn get_user(token: &str) -> AppResult<GhUser> {
    let resp = authed(http().get(format!("{API}/user")), token)
        .send()
        .await?;
    Ok(check(resp).await?.json().await?)
}

#[derive(Serialize)]
struct CreateRepoBody<'a> {
    name: &'a str,
    private: bool,
    description: Option<&'a str>,
    auto_init: bool,
}

#[derive(Deserialize)]
pub struct GhRepo {
    pub html_url: String,
    pub clone_url: String,
    pub default_branch: Option<String>,
}

pub async fn create_repo(
    token: &str,
    name: &str,
    private: bool,
    description: Option<&str>,
) -> AppResult<GhRepo> {
    let body = CreateRepoBody {
        name,
        private,
        description,
        auto_init: false,
    };
    let resp = authed(http().post(format!("{API}/user/repos")), token)
        .json(&body)
        .send()
        .await?;
    Ok(check(resp).await?.json().await?)
}

pub async fn get_repo(token: &str, owner: &str, repo: &str) -> AppResult<GhRepo> {
    let resp = authed(http().get(format!("{API}/repos/{owner}/{repo}")), token)
        .send()
        .await?;
    Ok(check(resp).await?.json().await?)
}

#[derive(Serialize)]
struct CreatePrBody<'a> {
    title: &'a str,
    head: &'a str,
    base: &'a str,
    body: &'a str,
}

#[derive(Deserialize)]
pub struct GhPrCreated {
    pub html_url: String,
    pub number: u64,
}

pub async fn create_pull(
    token: &str,
    owner: &str,
    repo: &str,
    title: &str,
    head: &str,
    base: &str,
    body: &str,
) -> AppResult<GhPrCreated> {
    let payload = CreatePrBody {
        title,
        head,
        base,
        body,
    };
    let resp = authed(
        http().post(format!("{API}/repos/{owner}/{repo}/pulls")),
        token,
    )
    .json(&payload)
    .send()
    .await?;
    Ok(check(resp).await?.json().await?)
}

// --- Sidebar lists ----------------------------------------------------------

#[derive(Deserialize)]
struct RawUser {
    login: String,
}

#[derive(Deserialize)]
struct RawItem {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    user: Option<RawUser>,
    /// Present on issues that are actually pull requests.
    pull_request: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhItem {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub author: Option<String>,
}

impl From<RawItem> for GhItem {
    fn from(r: RawItem) -> Self {
        GhItem {
            number: r.number,
            title: r.title,
            url: r.html_url,
            state: r.state,
            author: r.user.map(|u| u.login),
        }
    }
}

pub async fn list_pulls(token: &str, owner: &str, repo: &str) -> AppResult<Vec<GhItem>> {
    let resp = authed(
        http().get(format!(
            "{API}/repos/{owner}/{repo}/pulls?state=open&per_page=20"
        )),
        token,
    )
    .send()
    .await?;
    let raw: Vec<RawItem> = check(resp).await?.json().await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

// --- Account: your repos & automation runs ---------------------------------

#[derive(Deserialize)]
struct RawRepo {
    name: String,
    full_name: String,
    html_url: String,
    clone_url: String,
    #[serde(default = "default_main")]
    default_branch: String,
    private: bool,
}

fn default_main() -> String {
    "main".to_string()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    pub clone_url: String,
    pub default_branch: String,
    pub private: bool,
}

impl From<RawRepo> for RepoSummary {
    fn from(r: RawRepo) -> Self {
        RepoSummary {
            name: r.name,
            full_name: r.full_name,
            html_url: r.html_url,
            clone_url: r.clone_url,
            default_branch: r.default_branch,
            private: r.private,
        }
    }
}

// --- Account-wide automations (workflows across the user's repos) -----------

#[derive(Deserialize)]
struct RawWorkflow {
    id: u64,
    name: String,
    state: String,
    html_url: String,
}

#[derive(Deserialize)]
struct RawWorkflows {
    workflows: Vec<RawWorkflow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub repo_full_name: String,
    pub repo_name: String,
    pub name: String,
    pub id: u64,
    pub html_url: String,
    pub default_branch: String,
}

/// List every active workflow across the user's repos (bounded, fetched in
/// parallel) so the sidebar can show automations no matter which folder is open.
pub async fn list_automations(token: &str) -> AppResult<Vec<Automation>> {
    let repos = list_my_repos(token).await?;

    let mut handles = Vec::new();
    for repo in repos.into_iter().take(20) {
        let token = token.to_string();
        handles.push(tauri::async_runtime::spawn(async move {
            fetch_repo_automations(&token, repo).await
        }));
    }

    let mut out = Vec::new();
    for h in handles {
        if let Ok(Ok(mut v)) = h.await {
            out.append(&mut v);
        }
    }
    out.sort_by(|a, b| (&a.repo_name, &a.name).cmp(&(&b.repo_name, &b.name)));
    Ok(out)
}

async fn fetch_repo_automations(token: &str, repo: RepoSummary) -> AppResult<Vec<Automation>> {
    let resp = authed(
        http().get(format!(
            "{API}/repos/{}/actions/workflows?per_page=30",
            repo.full_name
        )),
        token,
    )
    .send()
    .await?;
    let raw: RawWorkflows = check(resp).await?.json().await?;
    Ok(raw
        .workflows
        .into_iter()
        .filter(|w| w.state == "active")
        .map(|w| Automation {
            repo_full_name: repo.full_name.clone(),
            repo_name: repo.name.clone(),
            name: w.name,
            id: w.id,
            html_url: w.html_url,
            default_branch: repo.default_branch.clone(),
        })
        .collect())
}

#[derive(Deserialize)]
struct RawRefObject {
    sha: String,
}
#[derive(Deserialize)]
struct RawRef {
    object: RawRefObject,
}

/// Resolve the commit SHA a branch points at (for tagging a release).
pub async fn branch_sha(token: &str, owner: &str, repo: &str, branch: &str) -> AppResult<String> {
    let resp = authed(
        http().get(format!("{API}/repos/{owner}/{repo}/git/ref/heads/{branch}")),
        token,
    )
    .send()
    .await?;
    let r: RawRef = check(resp).await?.json().await?;
    Ok(r.object.sha)
}

/// Create a lightweight tag ref (e.g. `refs/tags/v1.0.0`). Pushing the tag is
/// what triggers the release workflow that builds the installers. A duplicate
/// tag is reported as a friendly, specific error.
pub async fn create_tag(
    token: &str,
    owner: &str,
    repo: &str,
    tag: &str,
    sha: &str,
) -> AppResult<()> {
    let resp = authed(
        http().post(format!("{API}/repos/{owner}/{repo}/git/refs")),
        token,
    )
    .json(&serde_json::json!({ "ref": format!("refs/tags/{tag}"), "sha": sha }))
    .send()
    .await?;
    if resp.status().as_u16() == 422 {
        return Err(AppError::new(
            ErrorKind::Git,
            format!("The version “{tag}” already exists. Pick a new version number."),
        ));
    }
    check(resp).await?;
    Ok(())
}

/// Trigger a workflow_dispatch run of a workflow on the given ref (branch).
pub async fn run_workflow(
    token: &str,
    owner: &str,
    repo: &str,
    workflow_id: u64,
    git_ref: &str,
) -> AppResult<()> {
    let resp = authed(
        http().post(format!(
            "{API}/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches"
        )),
        token,
    )
    .json(&serde_json::json!({ "ref": git_ref }))
    .send()
    .await?;
    check(resp).await?;
    Ok(())
}

/// The authenticated user's own repositories, most recently updated first.
pub async fn list_my_repos(token: &str) -> AppResult<Vec<RepoSummary>> {
    let resp = authed(
        http().get(format!(
            "{API}/user/repos?per_page=50&sort=updated&affiliation=owner"
        )),
        token,
    )
    .send()
    .await?;
    let raw: Vec<RawRepo> = check(resp).await?.json().await?;
    Ok(raw.into_iter().map(Into::into).collect())
}

#[derive(Deserialize)]
struct RawRun {
    name: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    html_url: String,
}

#[derive(Deserialize)]
struct RawRuns {
    workflow_runs: Vec<RawRun>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub name: String,
    /// queued | in_progress | completed
    pub status: String,
    /// success | failure | cancelled | ... (only when completed)
    pub conclusion: Option<String>,
    pub url: String,
}

/// Recent GitHub Actions runs for a repo — the "did my automation run?" view.
pub async fn list_workflow_runs(
    token: &str,
    owner: &str,
    repo: &str,
) -> AppResult<Vec<WorkflowRun>> {
    let resp = authed(
        http().get(format!(
            "{API}/repos/{owner}/{repo}/actions/runs?per_page=15"
        )),
        token,
    )
    .send()
    .await?;
    let raw: RawRuns = check(resp).await?.json().await?;
    Ok(raw
        .workflow_runs
        .into_iter()
        .map(|r| WorkflowRun {
            name: r.name.unwrap_or_else(|| "Workflow".to_string()),
            status: r.status.unwrap_or_else(|| "unknown".to_string()),
            conclusion: r.conclusion,
            url: r.html_url,
        })
        .collect())
}

pub async fn list_issues(token: &str, owner: &str, repo: &str) -> AppResult<Vec<GhItem>> {
    let resp = authed(
        http().get(format!(
            "{API}/repos/{owner}/{repo}/issues?state=open&per_page=20"
        )),
        token,
    )
    .send()
    .await?;
    let raw: Vec<RawItem> = check(resp).await?.json().await?;
    // The issues endpoint also returns PRs; filter those out.
    Ok(raw
        .into_iter()
        .filter(|i| i.pull_request.is_none())
        .map(Into::into)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    /// The exact body GitHub served during the outage that produced the
    /// "GitHub returned an unexpected error" toast: a full HTML error page.
    const GITHUB_HTML_ERROR_PAGE: &str = r#"<!DOCTYPE html>
<html>
<!--
  Hello future GitHubber! I bet you're excited to DRY up these templates and make 'em
-->
<head><title>Server Error</title></head>
<body><div>Unicorn!</div></body>
</html>"#;

    #[test]
    fn html_error_page_is_summarized_not_dumped() {
        let out = describe_body(StatusCode::SERVICE_UNAVAILABLE, GITHUB_HTML_ERROR_PAGE);
        assert!(out.contains("503"));
        // The markup itself must not reach the UI.
        assert!(!out.contains("<!DOCTYPE"));
        assert!(!out.contains("Hello future GitHubber"));
        assert!(out.contains("outage"));
    }

    #[test]
    fn json_error_body_is_kept_verbatim() {
        let body = r#"{"message":"Not Found","status":"404"}"#;
        let out = describe_body(StatusCode::NOT_FOUND, body);
        assert_eq!(out, format!("HTTP 404 Not Found: {body}"));
    }

    #[test]
    fn long_body_is_truncated_and_marked() {
        let body = "x".repeat(5_000);
        let out = describe_body(StatusCode::BAD_GATEWAY, &body);
        assert!(
            out.chars().count() < 700,
            "detail was not capped: {}",
            out.chars().count()
        );
        assert!(out.ends_with('…'));
    }

    #[test]
    fn multibyte_body_truncates_without_panicking() {
        // Slicing this by byte index would panic mid-codepoint.
        let body = "é".repeat(5_000);
        let out = describe_body(StatusCode::BAD_GATEWAY, &body);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn empty_body_says_so() {
        assert!(describe_body(StatusCode::SERVICE_UNAVAILABLE, "   ").contains("empty"));
    }
}
