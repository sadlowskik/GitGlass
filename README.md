# GitGlass

**A visual file explorer with Git and GitHub built in — no command line required.**

Think *Windows File Explorer meets GitHub Desktop*, but prettier and more integrated. Every file and folder shows its Git status inline; staging, committing, pushing, and publishing to GitHub are one-click or right-click actions.

> Status: **Milestone 1 (Foundation)** — file explorer core, inline Git status (read-only), live file watching, and the full build/update/signing pipeline (signing stubbed). See [docs/ROADMAP.md](docs/ROADMAP.md).

---

## Tech stack

| Layer | Choice |
|---|---|
| Shell | Tauri v2 (Rust backend) |
| UI | React 18 + TypeScript + Tailwind CSS |
| State | Zustand |
| Git | `git2` (libgit2 bindings) — **never** shells out to the git CLI |
| GitHub | REST API + OAuth **device flow** (M4) |
| Secrets | OS keychain only — Windows Credential Manager / macOS Keychain (M4) |
| Updates | Tauri updater with signed manifests |

## Prerequisites

- **Node.js 20+** and npm
- **Rust** (stable) via [rustup](https://rustup.rs) — libgit2 is vendored & statically linked, so end users need no system git
- Windows: the **WebView2** runtime (auto-installed by the bundler) and, for building, the MSVC C++ build tools + CMake (required by vendored libgit2)

## Getting started

```bash
npm install
node scripts/gen-icons.mjs   # placeholder app icons (already generated in repo)
npm run app:dev              # launches the Tauri window with hot reload
```

Frontend-only (browser, no native shell):

```bash
npm run dev
```

## Scripts

| Command | What it does |
|---|---|
| `npm run app:dev` | Run the full desktop app (Tauri + Vite HMR) |
| `npm run app:build` | Produce MSI + NSIS installers |
| `npm run dev` | Frontend only in a browser |
| `npm test` | Frontend unit tests (Vitest) |
| `npm run lint` / `npm run typecheck` | Frontend quality gates |
| `cargo test` (in `src-tauri/`) | Rust tests, incl. Git status classification |

## Project layout

```
├─ src/                    React frontend
│  ├─ components/          UI (TitleBar, Sidebar, FileList, StatusBadge, ErrorState…)
│  ├─ lib/                 IPC wrappers, shared types, path utils (+ tests)
│  └─ store/              Zustand app store (navigation, theme, watcher wiring)
├─ src-tauri/             Rust backend
│  └─ src/
│     ├─ fs/              directory listing → DTOs
│     ├─ git/             libgit2 status engine (+ classification tests)
│     ├─ watcher.rs       debounced file watching → "fs:changed" events
│     ├─ error.rs         the single friendly-error type crossing IPC
│     └─ lib.rs           #[tauri::command] handlers
├─ .github/workflows/     CI (lint/test/build) + Release (signed installers)
├─ scripts/               icon generator + Windows signing stub
└─ docs/                  ROADMAP · ARCHITECTURE · SIGNING · SECURITY
```

## Security posture (already enforced in M1)

- **No raw Git errors reach the UI.** Every error crosses IPC as a friendly `AppError` (see `src-tauri/src/error.rs`); raw detail is hidden behind a disclosure.
- **Secrets never touch config.** Credentials will live only in the OS keychain (M4). `.env`, `*.pfx`, `*.pem`, `*.key` are git-ignored.
- The **secret scanner** (M3) will block commits containing keys/tokens — no one-click bypass.
- **Zero telemetry.** No analytics, no crash reporting. The app phones home only for the GitHub API and the signed update check.

See [docs/SECURITY.md](docs/SECURITY.md).

## License

Licensed under the [Apache License 2.0](LICENSE). See also [NOTICE](NOTICE).
