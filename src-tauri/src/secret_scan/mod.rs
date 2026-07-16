use std::cell::RefCell;
use std::path::Path;
use std::sync::OnceLock;

use git2::DiffOptions;
use regex::Regex;
use serde::Serialize;

use crate::error::AppResult;
use crate::git::ops::open_repo;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
}

/// One potential secret found in the staged diff. `preview` is always redacted
/// so the finding itself never becomes a place the secret leaks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Repo-relative path (forward slashes) for stable display.
    pub path: String,
    /// 1-based line number in the new file.
    pub line: u32,
    /// Human-readable rule name, e.g. "AWS Access Key".
    pub rule: String,
    pub severity: Severity,
    /// Redacted snippet, e.g. "AKIA…7 QWE".
    pub preview: String,
}

struct Rule {
    name: &'static str,
    severity: Severity,
    re: Regex,
}

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let r = |p: &str| Regex::new(p).expect("valid regex");
        vec![
            Rule {
                name: "AWS Access Key",
                severity: Severity::High,
                re: r(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
            },
            Rule {
                name: "GitHub Token",
                severity: Severity::High,
                re: r(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b"),
            },
            Rule {
                name: "GitHub Fine-grained Token",
                severity: Severity::High,
                re: r(r"\bgithub_pat_[A-Za-z0-9_]{60,}\b"),
            },
            Rule {
                name: "Google API Key",
                severity: Severity::High,
                re: r(r"\bAIza[0-9A-Za-z_\-]{35}\b"),
            },
            Rule {
                name: "Slack Token",
                severity: Severity::High,
                re: r(r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
            },
            Rule {
                name: "Stripe Live Key",
                severity: Severity::High,
                re: r(r"\b(?:sk|rk)_live_[A-Za-z0-9]{16,}\b"),
            },
            Rule {
                name: "npm Token",
                severity: Severity::High,
                re: r(r"\bnpm_[A-Za-z0-9]{36}\b"),
            },
            Rule {
                name: "API Key (sk-)",
                severity: Severity::Medium,
                re: r(r"\bsk-[A-Za-z0-9]{32,}\b"),
            },
        ]
    })
}

fn private_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----").unwrap()
    })
}

/// Matches `KEY = value`, `KEY: value`, `export KEY=value` and captures the
/// key name and the (possibly quoted) value.
fn assignment_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)\b([a-z0-9_]*(?:key|secret|token|password|passwd|pwd|api|auth|credential)[a-z0-9_]*)\s*[:=]\s*["']?([^"'\s]{8,})["']?"#).unwrap()
    })
}

/// Scan the currently staged changes for secrets. Only ADDED lines are examined
/// (what a commit would introduce), which keeps this well under the 2s budget
/// for typical commits.
pub fn scan_staged(repo_path: &Path) -> AppResult<Vec<Finding>> {
    let repo = open_repo(repo_path)?;
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let index = repo.index()?;

    let mut opts = DiffOptions::new();
    opts.context_lines(0).include_untracked(false);
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))?;

    let findings = RefCell::new(Vec::<Finding>::new());

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        None,
        Some(&mut |delta, _hunk, line| {
            // '+' = added line. Only new content can introduce a secret.
            if line.origin() != '+' {
                return true;
            }
            let path = delta
                .new_file()
                .path()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let lineno = line.new_lineno().unwrap_or(0);
            let content = String::from_utf8_lossy(line.content());
            scan_line(
                &path,
                lineno,
                content.trim_end(),
                &mut findings.borrow_mut(),
            );
            true
        }),
    )?;

    // Dedupe identical (path, line, rule) hits.
    let mut out = findings.into_inner();
    out.sort_by(|a, b| (&a.path, a.line, &a.rule).cmp(&(&b.path, b.line, &b.rule)));
    out.dedup_by(|a, b| a.path == b.path && a.line == b.line && a.rule == b.rule);
    Ok(out)
}

/// Directories skipped when scanning a whole folder — dependency/build/venv
/// noise that isn't the user's own code.
const SCAN_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "site-packages",
    ".next",
    ".cache",
    "vendor",
];
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_SCAN_FILES: usize = 3000;

/// Scan every file that *would* be published from `root` — a dry run so the user
/// can preview leaks before publishing. Respects .gitignore when the folder is a
/// repo; otherwise skips obvious dependency/build directories. Binary and very
/// large files are skipped.
pub fn scan_folder(root: &Path) -> AppResult<Vec<Finding>> {
    let repo = git2::Repository::discover(root).ok();
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut scanned = 0usize;

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if scanned >= MAX_SCAN_FILES {
                break;
            }
            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden and dependency dirs (dotfiles that are *files*,
                // like .env, are still scanned).
                if name.starts_with('.') || SCAN_SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                if is_ignored(repo.as_ref(), &path) {
                    continue;
                }
                stack.push(path);
            } else if ft.is_file() {
                if is_ignored(repo.as_ref(), &path) {
                    continue;
                }
                scan_file(&path, root, &mut out);
                scanned += 1;
            }
        }
    }

    out.sort_by(|a, b| (&a.path, a.line, &a.rule).cmp(&(&b.path, b.line, &b.rule)));
    out.dedup_by(|a, b| a.path == b.path && a.line == b.line && a.rule == b.rule);
    Ok(out)
}

fn is_ignored(repo: Option<&git2::Repository>, path: &Path) -> bool {
    match repo.and_then(|r| r.workdir().map(|w| (r, w))) {
        Some((r, wd)) => match path.strip_prefix(wd) {
            Ok(rel) => r.is_path_ignored(rel).unwrap_or(false),
            Err(_) => false,
        },
        None => false,
    }
}

fn scan_file(path: &Path, root: &Path, out: &mut Vec<Finding>) {
    // Skip very large files.
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
        return;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    // A NUL byte means binary — nothing to scan.
    if bytes.contains(&0) {
        return;
    }
    let rel = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string());
    let text = String::from_utf8_lossy(&bytes);
    for (i, line) in text.lines().enumerate() {
        scan_line(&rel, (i + 1) as u32, line, out);
    }
}

fn scan_line(path: &str, line: u32, content: &str, out: &mut Vec<Finding>) {
    // Skip comment lines — they're not live config.
    let trimmed = content.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with(';') {
        return;
    }

    // Provider-specific tokens (highest confidence).
    for rule in rules() {
        if let Some(m) = rule.re.find(content) {
            let hit = m.as_str();
            if is_allowlisted(hit) || is_allowlisted(content) {
                continue;
            }
            out.push(Finding {
                path: path.to_string(),
                line,
                rule: rule.name.to_string(),
                severity: rule.severity,
                preview: redact(hit),
            });
        }
    }

    // Private key material.
    if private_key_re().is_match(content) {
        out.push(Finding {
            path: path.to_string(),
            line,
            rule: "Private Key".to_string(),
            severity: Severity::High,
            preview: "-----BEGIN … PRIVATE KEY-----".to_string(),
        });
    }

    // `.env`-style / generic secret assignment with a high-entropy value.
    let is_env = is_dotenv(path);
    if let Some(caps) = assignment_re().captures(content) {
        let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        // A real credential has mixed case or digits; an all-lowercase,
        // word-with-underscores value is a placeholder phrase, not a secret.
        let tokenish = value
            .chars()
            .any(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
        if !is_allowlisted(value)
            && value.len() >= 12
            && (is_env || (tokenish && shannon_entropy(value) >= 3.5))
        {
            out.push(Finding {
                path: path.to_string(),
                line,
                rule: if is_env {
                    ".env Secret".to_string()
                } else {
                    "Secret Assignment".to_string()
                },
                severity: Severity::Medium,
                preview: redact(value),
            });
        }
    }
}

/// True for real dotenv files, but NOT the documentation variants that are meant
/// to be committed (`.env.example`, `.env.sample`, `.env.template`).
fn is_dotenv(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == ".env" {
        return true;
    }
    if let Some(rest) = name.strip_prefix(".env.") {
        return !matches!(rest, "example" | "sample" | "template" | "dist");
    }
    false
}

/// Filter obvious non-secrets: documented example keys, placeholders, and
/// Subresource-Integrity hashes. This is what keeps the false-positive rate low
/// enough to be trustworthy — a scanner that cries wolf gets ignored.
fn is_allowlisted(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    const NEEDLES: [&str; 12] = [
        "example",
        "placeholder",
        "changeme",
        "your_",
        "your-",
        "dummy",
        "sample",
        "redacted",
        "xxxx",
        "notreal",
        "<",
        "todo",
    ];
    if NEEDLES.iter().any(|n| lower.contains(n)) {
        return true;
    }
    // Environment-variable references are not literal secrets — they're the
    // *right* way to handle secrets. Covers PowerShell ($env:X, $X), shell
    // (${X}), Windows batch (%X%), and common getters (os.environ, getenv,
    // process.env, GitHub Actions ${{ secrets.X }}).
    if s.starts_with('$') || (s.starts_with('%') && s.ends_with('%')) {
        return true;
    }
    if lower.contains("os.environ")
        || lower.contains("getenv")
        || lower.contains("process.env")
        || lower.contains("secrets.")
    {
        return true;
    }
    // SRI / integrity hashes (common in lockfiles) are not secrets.
    if lower.starts_with("sha256-") || lower.starts_with("sha384-") || lower.starts_with("sha512-")
    {
        return true;
    }
    // A run of a single repeated character is a placeholder, not a key.
    if s.len() >= 8 {
        let first = s.as_bytes()[0];
        if s.bytes().all(|b| b == first) {
            return true;
        }
    }
    false
}

/// Shannon entropy in bits/char over the string's bytes.
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for b in s.bytes() {
        counts[b as usize] += 1;
    }
    let len = s.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Show just enough to recognise the value without reproducing it.
fn redact(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 8 {
        return "•".repeat(chars.len().max(4));
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 3..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn scan_folder_finds_secret_in_a_file() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("config.py"),
            "TOKEN = \"ghp_1234567890abcdefghijklmnopqrstuvwxyzAB\"\n",
        )
        .unwrap();
        let found = scan_folder(dir.path()).unwrap();
        assert!(found.iter().any(|f| f.rule == "GitHub Token"));
        assert_eq!(found[0].path, "config.py");
    }

    #[test]
    fn scan_folder_clean_folder_is_empty() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.py"), "print('hello world')\n").unwrap();
        assert!(scan_folder(dir.path()).unwrap().is_empty());
    }

    fn scan(content: &str) -> Vec<Finding> {
        let mut out = Vec::new();
        scan_line("config.txt", 1, content, &mut out);
        out
    }

    // --- True positives ---

    #[test]
    fn detects_github_token() {
        let hits = scan("token = ghp_1234567890abcdefghijklmnopqrstuvwxyzAB");
        assert!(hits.iter().any(|f| f.rule == "GitHub Token"));
    }

    #[test]
    fn detects_aws_access_key() {
        let hits = scan("aws_key=AKIA1234567890ABCDEF");
        assert!(hits.iter().any(|f| f.rule == "AWS Access Key"));
    }

    #[test]
    fn detects_private_key_header() {
        let hits = scan("-----BEGIN RSA PRIVATE KEY-----");
        assert!(hits.iter().any(|f| f.rule == "Private Key"));
    }

    #[test]
    fn detects_env_secret_in_dotenv() {
        let mut out = Vec::new();
        scan_line(".env", 3, "DATABASE_PASSWORD=s3cr3tP@ssw0rd12345", &mut out);
        assert!(out.iter().any(|f| f.rule == ".env Secret"));
    }

    #[test]
    fn preview_is_redacted_not_raw() {
        let hits = scan("token = ghp_1234567890abcdefghijklmnopqrstuvwxyzAB");
        let f = &hits[0];
        assert!(!f.preview.contains("1234567890abcdefghijklmnopqrstuvwxyz"));
        assert!(f.preview.contains('…'));
    }

    // --- False positives (must NOT flag) ---

    #[test]
    fn ignores_aws_documented_example_key() {
        // The canonical AWS docs example key.
        let hits = scan("aws_access_key_id = AKIAIOSFODNN7EXAMPLE");
        assert!(
            hits.is_empty(),
            "documented EXAMPLE key must not flag: {hits:?}"
        );
    }

    #[test]
    fn ignores_aws_documented_example_secret() {
        let hits = scan("aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY");
        assert!(
            hits.is_empty(),
            "documented EXAMPLE secret must not flag: {hits:?}"
        );
    }

    #[test]
    fn ignores_placeholder_value() {
        let hits = scan("API_KEY=your_api_key_here");
        assert!(hits.is_empty(), "placeholder must not flag: {hits:?}");
    }

    #[test]
    fn ignores_dotenv_example_file() {
        let mut out = Vec::new();
        scan_line(
            ".env.example",
            1,
            "SECRET_KEY=replace_me_with_real_value",
            &mut out,
        );
        assert!(out.is_empty(), ".env.example is documentation, not secrets");
    }

    #[test]
    fn ignores_sri_integrity_hash() {
        let hits = scan(
            "integrity: sha512-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789==",
        );
        assert!(hits.is_empty(), "SRI hash must not flag: {hits:?}");
    }

    #[test]
    fn ignores_comment_lines() {
        let hits = scan("# example: API_KEY=ghp_1234567890abcdefghijklmnopqrstuvwxyzAB");
        assert!(hits.is_empty(), "commented-out example must not flag");
    }

    #[test]
    fn low_entropy_prose_does_not_flag() {
        let hits = scan("password = please remember to change this later");
        assert!(hits.is_empty(), "prose sentence must not flag: {hits:?}");
    }
}
