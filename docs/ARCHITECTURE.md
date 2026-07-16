# Architecture

## Process model

```
┌─────────────────────────── Tauri app ───────────────────────────┐
│                                                                  │
│  WebView (React/TS)            │   Rust core                     │
│  ─────────────────            │   ─────────                     │
│  components/  store/  lib/     │   lib.rs (#[tauri::command])    │
│        │  invoke() / events    │        │                        │
│        └──────────────────────►│   fs/   → directory DTOs        │
│        ◄───── "fs:changed" ────┤   git/  → libgit2 status        │
│                                │   watcher.rs → debounced notify │
│                                │   error.rs  → AppError (only    │
│                                │               shape crossing IPC)│
└──────────────────────────────────────────────────────────────────┘
```

**One rule at the boundary:** the frontend only ever receives serde DTOs
(`DirListingDto`, `RepoInfoDto`, `DirEntryDto`) or a single `AppError`. It never
sees a `git2::Error`, an `io::Error`, or a raw path type.

## Frontend

- **State** lives in one Zustand store (`store/useAppStore.ts`): current listing,
  loading/error, theme, sidebar (pinned/recent), and the file-watch subscription.
  Only preferences + sidebar are persisted (`partialize`).
- **IPC** is funneled through `lib/tauri.ts::call()`, the single place errors are
  normalized. Adding a backend command = one typed wrapper here.
- **Types** in `lib/types.ts` mirror the Rust DTOs (camelCase via serde rename).
- **Components** are presentational; they read/write the store. `ErrorState` and
  `StatusBadge` centralize the two things that must look consistent everywhere:
  error UX and status color.

## Backend

- **`git/status.rs`** is the core of M1. `RepoContext::discover` finds the repo
  and computes the full status map **once** per listing. `classify()` collapses
  git2's status bitflags into one user-facing `GitStatus` in a fixed priority
  order (conflict > staged > modified > untracked > ignored > clean). Directory
  roll-up (`dir_status`) reports whether a folder contains actionable changes.
- **`fs/mod.rs`** reads the directory and joins each entry to its status. Git is
  computed once and shared across all entries — no per-file repo open.
- **`watcher.rs`** watches the *current* repo root (re-armed on navigation),
  debounces bursts (300ms), ignores `.git` internals, and emits `fs:changed`.
  The UI refreshes only if it's inside the changed root.

## Why these choices

- **libgit2, never the CLI:** deterministic behavior, no dependency on a
  system git, structured errors, and no shell-injection surface.
- **vendored-libgit2:** statically linked so end users install nothing.
- **Compute-status-once:** status for a 1,000-file directory is one libgit2
  status walk, not 1,000 lookups — this is what keeps navigation instant and
  gives M3's scanner room inside its 2s budget.

## Extension points (later milestones)
- `git/` gains `stage.rs`, `commit.rs`, `remote.rs`, `diff.rs`, `branch.rs`.
- `secret_scan/` (M3) reads the staged diff via libgit2 and runs detectors.
- `github/` (M4) holds the device-flow client; tokens go to the keychain via a
  `credentials` module wrapping the OS store.
