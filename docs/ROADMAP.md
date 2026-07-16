# GitGlass Roadmap

Milestones are cumulative. **No feature ships without its error states designed.** The build/update/signing pipeline is proven in M1 so every later milestone ships through a trusted channel.

## M1 — Foundation (this milestone) ✅ scaffolded
- [x] Tauri v2 + React + Tailwind shell, custom title bar, dark/light theme
- [x] File explorer core: browse, breadcrumbs, file/folder icons, sidebar (pinned/recent)
- [x] Automatic Git repo detection (`Repository::discover`)
- [x] Inline Git status badges (staged/modified/untracked/ignored/conflict) — **read-only**
- [x] Folder status roll-up ("contains changes")
- [x] Live updates via debounced file watching (`notify`)
- [x] Friendly-error type + `ErrorState` UI (seed of M6)
- [x] Pipeline: Tauri updater wired, CI (lint/test/build), Release workflow → MSI/NSIS, **Windows signing stubbed**
- [x] Tests: Git status classification (Rust), path utils (frontend)

## M2 — Git actions ✅ built
- [x] Stage/unstage via checkbox + right-click context menu; animated badge/checkbox
- [x] Commit panel sliding up from the bottom ("Save N changes", ⌘/Ctrl+Enter)
- [x] Stage all / Unstage all; repo-wide staged/unstaged counts
- [x] Push/pull toolbar with live "↑N ahead / ↓M behind" (local upstream tracking)
- [x] Toast system for action feedback; errors keep the never-show-raw contract
- [x] Error states: empty message, nothing-to-commit, no-identity, no-upstream,
      non-fast-forward → "pull first", auth-failed (friendly)
- [x] Tests: stage→commit roundtrip, unstage, nothing-staged, empty message,
      path relativize (false-prefix guard, repo-root → match-all)
- Notes: push/pull network auth bridges through the user's existing Git
  credential helper / SSH agent until first-class GitHub OAuth (M4). Pull is
  fast-forward only; a diverged history returns a friendly "merging coming
  soon" — real merge lands later.

## M3 — Secret protection (non-negotiable) ✅ built
- [x] Pre-commit scan of the **staged diff** (added lines only): AWS/GitHub/Google/Slack/Stripe/npm tokens, private-key blocks, `.env` values, high-entropy secret assignments
- [x] Commit **blocked in the backend** (defense in depth) with a dialog: exact file + line, **redacted** preview
- [x] Three outs: cancel / add-to-`.gitignore`+unstage / **typed-confirmation** override ("commit anyway") — no one-click bypass
- [x] Allowlist for documented examples (`AKIA…EXAMPLE`), placeholders, `.env.example`, SRI hashes → low false positives
- [x] Scans added lines only → well within the **< 2s** budget
- [x] Tests: true positives, documented-example false positives, redaction, comment/prose/`.env.example` non-flagging (12 scanner tests)

## M4 — GitHub integration ✅ built
- [x] OAuth **device flow** sign-in (no pasted tokens); token stored **only** in Windows Credential Manager via `keyring`
- [x] Connected-state accent (GitHub green) in title bar + toolbar
- [x] Publish a local folder to a new GitHub repo (create + set origin + push)
- [x] Clone by URL; open a pull request from the current branch
- [x] Open PRs & issues in the sidebar (click → opens in browser)
- [x] Friendly error states: not-configured, device-code expiry/denied, network, 401/403/404/422, on-default-branch, not-a-GitHub-remote
- Notes: requires a GitHub OAuth **App client id** — see [GITHUB.md](GITHUB.md). git2 `https` enabled (WinHTTP/SChannel) so push/clone work. Richer PR views + merge-on-pull come later.

## M5 — Diff viewer, branches, error translation ✅ built
- [x] Diff viewer: **unified + side-by-side** toggle, syntax highlighting
      (highlight.js, theme-aware tokens), per-file change vs HEAD (staged +
      unstaged), binary + empty states. Opens on file double-click / "View changes".
- [x] Visual branch switcher (chip → menu): list, switch, create+checkout,
      delete; **dirty-tree warning** before switching, and the backend uses a
      *safe* checkout so nothing is discarded.
- [x] Friendly-error layer complete: every backend path returns `AppError`;
      surfaced via `ErrorState` (navigation) or toasts with a "details"
      disclosure — no raw git strings anywhere.
- [x] Tests: branch create/switch/delete roundtrip, duplicate-name error
      (30 Rust tests total).

## Shippable (v1.0)
- Real Windows Authenticode cert wired into CI; **macOS notarization**
- **Zero telemetry** (product decision, 2026-07-12): no analytics, no crash
  reporting. Network calls limited to GitHub API + the signed update check.
- First-run onboarding: connect GitHub + pin a folder in **< 60s**
- **Updater signature-verification tests**; auto-update proven on a real tagged release

## Where signing & updates slot in
- **Mechanism** (updater plugin, signed manifest, release job, sign script) → **M1**, stubbed/unsigned.
- **Real cert + notarization** → **Shippable**, after the pipeline has run for four milestones.
