# Security & privacy

## Principles

1. **Credentials live only in the OS keychain.** Windows Credential Manager /
   macOS Keychain. Never in app config, never in `.env`, never in plaintext,
   never logged. (Wired in M4; the boundary is designed for it now.)
2. **Secrets never get committed by accident.** The M3 pre-commit scanner
   inspects the *staged diff* for API keys, tokens, private keys, `.env`
   contents, and high-entropy strings, and **blocks** the commit. Override
   requires typed confirmation — there is no one-click bypass.
3. **No raw Git errors, ever.** Everything crossing IPC is a friendly `AppError`
   (`src-tauri/src/error.rs`). Raw text is available only behind an explicit
   "technical details" disclosure.
4. **Zero telemetry — by product decision.** GitGlass ships with **no**
   analytics and **no** crash reporting. The app makes network requests only
   where the user explicitly drives them: the GitHub REST API (M4) and the
   signed update check. There is no background phone-home. Any future change to
   this posture requires explicit, opt-in sign-off and would be documented here.

## Secret scanner design (M3 — implemented)

- **Enforced in the backend.** The `commit` command re-scans and refuses unless
  `allow_secrets` is set, which only happens after the typed override — so there
  is no bypass even for a caller that skips the UI pre-scan.
- Detectors: provider-prefixed keys (AWS, GitHub, Google, Slack, Stripe, npm,
  `sk-`), PEM private-key blocks, `.env` assignment lines, and a Shannon-entropy
  heuristic for generic secret assignments (gated to token-like values).
- **False-positive care:** documented example keys (`AKIAIOSFODNN7EXAMPLE`),
  placeholders, `.env.example`/`.sample`/`.template`, and SRI hashes are
  allow-listed and covered by tests.
- **Redaction:** findings show a redacted preview (`AKIA…7QW`), never the raw
  value — the finding is not a new place the secret leaks.
- **Performance:** scans added diff lines only; well within **< 2s** for typical
  commits.
- **Findings UI:** exact file + line, with cancel / add-to-`.gitignore` /
  typed "commit anyway" override.

## GitHub token storage (M4 — implemented)

The OAuth device-flow token lives **only** in the OS keychain (Windows
Credential Manager) via `keyring`, under service `GitGlass` / account
`github-token`. It is never written to config or `.env`, never logged, and is
removed on sign-out. See [GITHUB.md](GITHUB.md).

## Content Security Policy

The webview runs under a strict CSP (`tauri.conf.json → app.security.csp`):
`default-src 'self'` with no remote script or style origins. All assets are
bundled; nothing is fetched from a CDN.

## Reporting

Security issues: please open a private advisory rather than a public issue.
