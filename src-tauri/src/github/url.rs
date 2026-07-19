//! Deciding whether a remote URL is really GitHub.
//!
//! This is a security boundary, not a convenience helper: the answer decides
//! whether the user's GitHub token — which carries `repo` and `workflow` scope
//! — is handed to whatever host is on the other end. Substring tests like
//! `url.contains("github.com")` are not a host check; every URL in the
//! `rejects_look_alike_hosts` test below passes such a test while resolving to
//! an attacker's server. Always compare the parsed host.

use git2::{Cred, CredentialType};
use url::Url;

/// The only host we will send the stored token to.
fn is_github_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("github.com")
}

/// Whether `url` is a GitHub URL that may receive the stored token.
///
/// Requires HTTPS: the token travels as an HTTP Basic password, so cleartext
/// `http://github.com/...` is refused rather than leaked to the network.
pub fn is_github_token_url(url: &str) -> bool {
    match Url::parse(url.trim()) {
        Ok(u) => u.scheme() == "https" && u.host_str().is_some_and(is_github_host),
        Err(_) => false,
    }
}

/// A credentials callback that gives the token to GitHub over HTTPS and to
/// nobody else.
///
/// The host is re-checked on every invocation rather than once up front,
/// because libgit2 follows HTTP redirects and calls back with the URL it
/// actually reached — a github.com URL that redirects off-site must not carry
/// the token with it.
pub fn token_credentials(
    token: String,
) -> impl FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, git2::Error> {
    move |url, _user, _allowed| {
        if !is_github_token_url(url) {
            return Err(git2::Error::from_str(&format!(
                "refusing to send GitHub credentials to non-GitHub URL: {url}"
            )));
        }
        // Git transport uses HTTP Basic (not the REST Bearer header). GitHub's
        // documented form is username "x-access-token" with the token as the
        // password; sending the token as the *username* is rejected for OAuth
        // tokens, which silently broke push while REST calls still worked.
        Cred::userpass_plaintext("x-access-token", &token)
    }
}

/// Parse `owner` and `repo` out of a GitHub remote URL.
///
/// Accepts the forms git actually writes into `.git/config`: `https://`,
/// `ssh://`, and the scp-like `git@github.com:owner/repo.git`. Anything whose
/// host isn't github.com returns `None`, so we never aim an API call at a repo
/// path taken from someone else's server.
///
/// Unlike [`is_github_token_url`] this tolerates `http://`, because the result
/// only identifies which repo to ask api.github.com about — no credential rides
/// on the URL itself.
pub fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    let raw = url.trim();
    // scp-like syntax isn't a URL and won't parse; recognise it separately.
    if let Some(path) = scp_like_path(raw) {
        return split_owner_repo(path);
    }
    let parsed = Url::parse(raw).ok()?;
    if !matches!(parsed.scheme(), "https" | "http" | "ssh" | "git") {
        return None;
    }
    if !parsed.host_str().is_some_and(is_github_host) {
        return None;
    }
    split_owner_repo(parsed.path())
}

/// Match `[user@]host:path`, the scp-like remote syntax, returning `path` only
/// when `host` is GitHub. A `scheme://` URL is never scp-like.
fn scp_like_path(raw: &str) -> Option<&str> {
    if raw.contains("://") {
        return None;
    }
    let (authority, path) = raw.split_once(':')?;
    // Anything before '@' is userinfo; the host is what follows the last one.
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    is_github_host(host).then_some(path)
}

/// Split a `/owner/repo(.git)` remote path. Rejects extra path segments: a real
/// remote URL has exactly two, and a browser URL like `/owner/repo/tree/main`
/// would otherwise yield a repo name of `repo/tree/main`.
fn split_owner_repo(path: &str) -> Option<(String, String)> {
    let (owner, repo) = path.trim_matches('/').split_once('/')?;
    // `strip_suffix`, not `trim_end_matches`, which would strip repeatedly and
    // turn the legitimate repo name `foo.git.git` into `foo`.
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_real_github_https_urls() {
        assert!(is_github_token_url("https://github.com/owner/repo.git"));
        assert!(is_github_token_url("https://github.com:443/owner/repo.git"));
        // Host comparison is case-insensitive, per DNS.
        assert!(is_github_token_url("https://GitHub.COM/owner/repo.git"));
    }

    /// Every URL here contains the literal text "github.com" and so defeated the
    /// old `url.contains("github.com")` check, while resolving elsewhere.
    #[test]
    fn rejects_look_alike_hosts() {
        for url in [
            // Userinfo trick: the real host is evil.example.
            "https://github.com@evil.example/owner/repo.git",
            "https://github.com:x@evil.example/owner/repo.git",
            // Suffix and prefix confusion.
            "https://github.com.evil.example/owner/repo.git",
            "https://notgithub.com/owner/repo.git",
            // github.com appears only in the path.
            "https://evil.example/github.com/owner/repo.git",
        ] {
            assert!(
                !is_github_token_url(url),
                "token would have been sent to: {url}"
            );
            assert_eq!(parse_owner_repo(url), None, "parsed as GitHub: {url}");
        }
    }

    #[test]
    fn rejects_cleartext_http_for_token() {
        // The token is an HTTP Basic password; http:// would put it on the wire.
        assert!(!is_github_token_url("http://github.com/owner/repo.git"));
    }

    #[test]
    fn rejects_unparseable_urls_for_token() {
        assert!(!is_github_token_url(""));
        assert!(!is_github_token_url("github.com/owner/repo"));
        // SSH carries no token — the agent authenticates instead.
        assert!(!is_github_token_url("git@github.com:owner/repo.git"));
    }

    #[test]
    fn token_callback_refuses_non_github_urls() {
        let mut cb = token_credentials("secret-token".to_string());
        assert!(cb(
            "https://evil.example/owner/repo.git",
            None,
            CredentialType::USER_PASS_PLAINTEXT
        )
        .is_err());
        assert!(cb(
            "https://github.com/owner/repo.git",
            None,
            CredentialType::USER_PASS_PLAINTEXT
        )
        .is_ok());
    }

    #[test]
    fn token_callback_error_does_not_leak_the_token() {
        let mut cb = token_credentials("secret-token".to_string());
        // `Cred` isn't Debug, so unwrap the error by hand rather than expect_err.
        let Err(err) = cb(
            "https://evil.example/x.git",
            None,
            CredentialType::USER_PASS_PLAINTEXT,
        ) else {
            panic!("must refuse a non-GitHub URL");
        };
        assert!(!err.message().contains("secret-token"));
    }

    #[test]
    fn parses_every_remote_form_git_writes() {
        let want = Some(("owner".to_string(), "repo".to_string()));
        for url in [
            "https://github.com/owner/repo.git",
            "https://github.com/owner/repo",
            "https://github.com/owner/repo/",
            "http://github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
            "  https://github.com/owner/repo.git  ",
        ] {
            assert_eq!(parse_owner_repo(url), want, "failed on: {url}");
        }
    }

    #[test]
    fn rejects_paths_that_are_not_owner_repo() {
        assert_eq!(parse_owner_repo("https://github.com/owner"), None);
        assert_eq!(parse_owner_repo("https://github.com/"), None);
        // A browser URL, not a remote — must not yield repo = "repo/tree/main".
        assert_eq!(
            parse_owner_repo("https://github.com/owner/repo/tree/main"),
            None
        );
    }

    #[test]
    fn strips_only_one_dot_git_suffix() {
        assert_eq!(
            parse_owner_repo("https://github.com/owner/foo.git.git"),
            Some(("owner".to_string(), "foo.git".to_string()))
        );
    }

    #[test]
    fn windows_path_is_not_mistaken_for_scp_syntax() {
        assert_eq!(parse_owner_repo(r"C:\Users\me\repo"), None);
    }

    /// End-to-end guard, driving a real libgit2 fetch against a local server
    /// that asks for Basic auth: the token must never reach the wire when the
    /// host isn't GitHub. Asserting on the bytes the server received is what
    /// makes this meaningful — the previous code sent the token here.
    #[test]
    fn token_never_reaches_a_non_github_server() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = stream.unwrap();
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
                // Demand Basic auth, the prompt that makes libgit2 ask us for
                // credentials in the first place.
                let _ = s.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\n\
                      WWW-Authenticate: Basic realm=\"git\"\r\n\
                      Content-Length: 0\r\n\r\n",
                );
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let mut remote = repo
            .remote("origin", &format!("http://127.0.0.1:{port}/owner/repo.git"))
            .unwrap();

        let mut cbs = git2::RemoteCallbacks::new();
        cbs.credentials(token_credentials("secret-token".to_string()));
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(cbs);

        // Expected to fail: we refuse to authenticate to this host.
        assert!(remote.fetch(&["main"], Some(&mut fo), None).is_err());

        let requests: Vec<String> = rx.try_iter().collect();
        assert!(!requests.is_empty(), "server saw no request at all");
        for req in &requests {
            assert!(
                !req.to_lowercase().contains("authorization"),
                "credentials were sent to a non-GitHub host:\n{req}"
            );
            assert!(
                !req.contains("secret-token"),
                "raw token on the wire:\n{req}"
            );
            // base64("x-access-token:secret-token")
            assert!(
                !req.contains("eC1hY2Nlc3MtdG9rZW46c2VjcmV0LXRva2Vu"),
                "base64 token on the wire:\n{req}"
            );
        }
    }
}
