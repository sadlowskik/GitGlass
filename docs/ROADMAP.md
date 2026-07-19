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
- [x] **No-identity is recoverable in-app** (2026-07-16): the M2 error *state*
      existed, but its only remedy was `git config` in a terminal — a dead end
      for a "no command line required" product. A failed save now opens a dialog
      that writes name/email to the **global** Git config via git2 and resumes
      the commit. Prefilled from the GitHub login + `@users.noreply.github.com`.
      Reminder that an error state isn't designed until it has a way *out*.
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

## M5.1 — Sync hardening & operation parity (2026-07-17) ✅ built
Closing gaps found in a Git/GitHub audit. Each shipped with tests.
- [x] **Pull no longer discards uncommitted work.** It was doing a *force*
      checkout on fast-forward, silently overwriting local edits (proven with a
      repro). Now a **safe** checkout that refuses — "commit or discard first" —
      exactly like `git pull`. Regression tests: clean FF, conflicting edit
      refused+preserved, unrelated edit preserved.
- [x] **Fetch** (manual, toolbar button): refreshes remote-tracking refs so
      "N behind" is real without a pull. No background auto-fetch — that would
      break the [zero-background-network](SECURITY.md) promise; fetch is
      user-driven.
- [x] **Discard changes** (file context menu → confirm dialog): restores a file
      to HEAD (reverts edits, restores deletions). Untracked files are left alone.
- [x] **Amend last commit** (commit panel): folds staged changes into the last
      commit; blank message keeps the original. **Refuses once the commit is on
      the remote** (won't rewrite shared history).
- [x] **Tag sync:** push publishes local tags (best-effort); fetch/pull download
      tags — so version/release tags are visible both ways.
- [x] **Connect to existing GitHub repo:** sets `origin` to an existing repo
      (Publish only ever *created* a new one). URL validated as GitHub.
- [x] **Sign-in survives network blips:** a transient API failure keeps you
      signed in with your cached identity; only a real 401/403 shows signed-out.
- Still deferred (by design): in-app **merge/conflict resolution** for diverged
      pulls stays a friendly error for now.

## Shippable (v1.0)
- Real Windows Authenticode cert wired into CI; **macOS notarization**
- **Zero telemetry** (product decision, 2026-07-12): no analytics, no crash
  reporting. Network calls limited to GitHub API + the signed update check.
- First-run onboarding: connect GitHub + pin a folder in **< 60s**
- **Updater signature-verification tests**; auto-update proven on a real tagged release

## Where signing & updates slot in
- **Mechanism** (updater plugin, signed manifest, release job, sign script) → **M1**, stubbed/unsigned.
- **Real cert + notarization** → **Shippable**, after the pipeline has run for four milestones.
