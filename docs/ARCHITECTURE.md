# Architecture

Describes the code as it exists on branch `m5.1-sync-hardening` (working tree,
including uncommitted changes). Where this document and a code comment or another
doc disagree, see [Drift](#drift) — the code is what's recorded here.

## What this is

GitGlass is a Tauri v2 desktop app: a file explorer whose rows carry Git status,
with staging, committing, branch switching, sync (fetch/pull/push), GitHub
publish/clone/PR, and scheduled-Python GitHub Actions authoring built in. The
product premise is that the user never opens a terminal, so every Git operation
goes through libgit2 (`git2`) rather than shelling out to `git`, and every error
crossing the IPC boundary is rewritten into a sentence a non-developer can act on.

## Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri v2 (`src-tauri/Cargo.toml:20`) | custom titlebar, `decorations: false` (`src-tauri/tauri.conf.json:16`) |
| Backend | Rust 2021, MSRV 1.77 | lib + thin `main.rs` for mobile reuse (`Cargo.toml:12-15`) |
| Git | `git2` 0.19, `vendored-libgit2` + `https` + `vendored-openssl` (`Cargo.toml:33`) | statically linked; no system git required |
| HTTP | `reqwest` 0.13 (`Cargo.toml:48`) | resolves to **rustls**, not native-tls — see Drift |
| Credentials | `keyring` 3, `windows-native` + `apple-native` (`Cargo.toml:56`) | |
| Watching | `notify` 6 + `notify-debouncer-full` 0.3 | |
| Frontend | React 18, TypeScript 5.6, Vite 5, Zustand 5, Tailwind 3 | `package.json` |
| Tests | `cargo test` (inline `#[cfg(test)]` modules), Vitest for TS | |

Version is `0.2.1`, and must be kept identical in **four** places:
`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and the
`gitglass` entry in `src-tauri/Cargo.lock` (refreshed by any `cargo` command
after bumping `Cargo.toml`). `package-lock.json` carries it twice and is updated
by `npm version`. `tauri.conf.json` is the one the installers and the updater
endpoint URL are built from.

Node **20.19+ or 22.12+** is required — Vite 7 and `@vitejs/plugin-react` 5
declare that engine range, and CI pins Node 22. `package.json` declares
`engines.node` so a too-old local Node fails early rather than mid-build.

## Shape

```
React WebView                          Rust core
─────────────                          ─────────
components/*.tsx                       lib.rs          40 #[tauri::command] fns
   │  read/write                          │            (invoke_handler, lib.rs:359)
   ▼                                      │
store/useAppStore.ts   ── invoke ──►  fs/mod.rs   ──► git/status.rs  (one status walk)
store/useGithubStore.ts               git/{ops,sync,branch,diff}.rs
   │                                  github/{auth,api,remote,url}.rs
   │  ◄──── "fs:changed" event ────   secret_scan/mod.rs
   ▼                                  automation/mod.rs
lib/tauri.ts  (single call() wrapper)  watcher.rs  ──► emits "fs:changed"
lib/types.ts  (mirrors the serde DTOs)  error.rs  ──► AppError, the only error shape
```

Two invariants hold the boundary together:

1. **One error shape.** Every command returns `AppResult<T>` = `Result<T, AppError>`
   (`src-tauri/src/error.rs:101`). `AppError` (`error.rs:8`) has a user-friendly
   `message`, a machine-readable `kind`, and an optional `detail` holding the raw
   `git2`/`io`/`reqwest`/`keyring` text. `From` impls at `error.rs:57,69,81,91`
   guarantee no raw error ever becomes the primary message.
2. **One IPC funnel.** All 40 commands are wrapped in `src/lib/tauri.ts:46-170`,
   each going through `call()` (`tauri.ts:26`), which normalizes anything
   non-`AppError`-shaped into `{kind: "unknown", …}` (`tauri.ts:34`).

## Entry points

`run()` at `src-tauri/src/lib.rs:346` registers three plugins (dialog, updater,
opener), manages `WatchState`, and shows the `main` window. Webview permissions
are in `src-tauri/capabilities/default.json` — window controls, events, dialog
open, updater, opener.

### Command surface

Grouped as the handler list orders them (`lib.rs:359-401`). "TS" is the
`api.*` name in `src/lib/tauri.ts`.

| Command | TS | Rust impl | Notes |
|---|---|---|---|
| `list_dir` | `listDir` | `lib.rs:20` → `fs::list_dir` | also re-arms the watcher |
| `home_dir` | `homeDir` | `lib.rs:35` | |
| `reveal_in_os` | `revealInOs` | `lib.rs:41` | `opener::reveal_item_in_dir` |
| `stage_paths` / `unstage_paths` | `stagePaths`/`unstagePaths` | `git/ops.rs:303,318` | |
| `commit` | `commit` | `lib.rs` → `git/ops.rs:441` | `secret_scan::gate` |
| `scan_staged` / `scan_folder` | `scanStaged`/`scanFolder` | `secret_scan/mod.rs:109,179` | |
| `ignore_paths` | `ignorePaths` | `git/ops.rs:492` | appends + unstages |
| `git_identity` / `set_git_identity` | `gitIdentity`/`setGitIdentity` | `git/ops.rs:18,39` | global config |
| `init_repo` | `initRepo` | `git/ops.rs:152` | offline; secret-gated |
| `pull` / `push` / `fetch` | same | `git/sync.rs:128,190,259` | |
| `discard_paths` | `discardPaths` | `git/ops.rs:350` | destructive |
| `amend_commit` | `amendCommit` | `lib.rs` → `git/ops.rs:381` | `secret_scan::gate`; refuses if pushed |
| `file_diff` | `fileDiff` | `git/diff.rs:39` | staged+unstaged vs HEAD; capped at 20k lines |
| `list_branches` / `create_branch` / `switch_branch` / `delete_branch` | same | `git/branch.rs:19,43,74,97` | |
| `list_python_files` / `list_workflows` / `automation_context` / `create_workflow` | `listPythonFiles`/`listWorkflows`/`hasRequirements`/`createWorkflow` | `automation/mod.rs` | |
| `github_status` / `github_start_login` / `github_poll_login` / `github_sign_out` | `githubStatus`/`githubStartLogin`/`githubPollLogin`/`githubSignOut` | `github/auth.rs:190,105,136,220` | device flow |
| `github_publish` | `githubPublish` | `github/remote.rs:259` | |
| `github_clone` | `githubClone` | `github/remote.rs:307` | |
| `github_set_origin` | `githubSetOrigin` | `github/remote.rs:134` | connects, creates nothing |
| `github_open_pr` / `github_list_prs` / `github_list_issues` | `githubOpenPr`/`githubListPrs`/`githubListIssues` | `github/remote.rs:329,354,360` | |
| `github_my_repos` / `github_workflow_runs` / `github_automations` / `github_run_now` | `githubMyRepos`/`githubWorkflowRuns`/`githubAutomations`/`githubRunNow` | `github/remote.rs:367,373,380,386` | |
| `github_create_release` | `githubCreateRelease` | `github/remote.rs:397` | tags default branch |

### Events

One event only: `fs:changed`, payload = the watched repo root as a string,
emitted from `watcher.rs:42`. Subscribed via `onFsChanged` (`tauri.ts:177`) and
wired in `useAppStore.ts:419` (`rewatch`).

## Key paths

### Listing a directory (the hot path)

`useAppStore.navigate` (`useAppStore.ts:105`) → `api.listDir` → `list_dir`
(`lib.rs:20`) → `fs::list_dir` (`fs/mod.rs:47`).

`RepoContext::discover` (`git/status.rs:41`) runs **one** libgit2 status walk for
the whole repo and builds an absolute-path→`GitStatus` map (`status.rs:173`), so a
1,000-entry directory costs one walk rather than 1,000 lookups. `classify`
(`status.rs:196`) collapses git2 bitflags in fixed priority: conflict > staged >
modified > untracked > ignored > clean.

Directories get a roll-up via `dir_status` (`status.rs`) — "does any descendant
have an actionable change". This is an O(1) hash lookup into a `dir_rollup` map
built once per listing by `roll_up_dirs`, which folds each status entry into
itself and every ancestor up to the repo root. It used to scan the whole status
map per subdirectory, which made a single listing O(directories × entries):
measured at ~240 ms for a 50k-entry repo with 20 subfolders, on a code path the
file watcher can fire every 300 ms. The entry's *own* path is included in the
fold deliberately — that self-match is what makes an ignored directory git
reports as a single entry (`node_modules/`) render gray, and the unit tests in
`status.rs` pin it.

Ahead/behind is computed from local refs only, no network (`status.rs:133`), so
it is stale until a `fetch`.

Path map keys are normalized (`status.rs:231`): separators unified, `\\?\`
verbatim prefix stripped, trailing separator trimmed. **Case is deliberately not
normalized** — the same choice is made independently in `git/ops.rs:247`, and both
rely on UI paths sharing an origin with the repo workdir.

### Watching

After the listing returns, `list_dir` re-arms the watcher on the repo root, or
calls `WatchState::clear` when the path isn't in a repo (`lib.rs:29`). That
`clear` matters: without it, navigating out of a repo left the previous root
watched — OS handle and debouncer thread included — with no subscriber.

`WatchState::watch` (`watcher.rs`) is a no-op if the root is unchanged, else it
replaces the debouncer (300 ms) and emits `fs:changed`. The frontend refreshes
only if the changed root matches the one it's watching (`useAppStore.ts`).

Two things about this path are easy to get wrong:

- **`is_relevant` is a performance boundary.** Every event that survives it costs
  a full libgit2 status walk *and* a full file-list re-render. A build writing
  into `target/` (24k files in this repo) would otherwise drive that loop
  continuously for the build's whole duration. Filtering happens on path
  components, not substrings, so `environment/` isn't skipped for containing
  `env` — there are tests for exactly this.
- **The debouncer uses `NoCache`, not the default `FileIdMap`.** The map exists
  to correlate rename events and resync after dropped ones; this watcher uses
  neither — it only asks "did anything relevant change under this root". Carrying
  it was a slow leak: `add_path` inserts on every Create and prunes only on
  Remove, so a build emitting content-hashed artifact names grew the map for the
  life of the watch. (It was also misconfigured: registration went through
  `d.watcher().watch(..)`, which never calls `add_root`, so the cache had no
  roots and could neither prune nor rescan.)

### Staging and committing

`stage_paths` / `unstage_paths` take **absolute** paths from the UI;
`relativize` (`git/ops.rs:261`) converts them to repo-relative pathspecs and
silently skips anything outside the workdir. It guards against false prefix
matches (`/repo` vs `/repo2/file`, `ops.rs:280`) and maps the repo root itself to
the `*` pathspec (`ops.rs:279`). Staging uses `add_all` semantics so deletions
stage correctly (`ops.rs:310`). Unstage resets to HEAD, or drops index entries
outright on an unborn branch (`ops.rs:330`).

The commit flow has two stages and two scans:

1. `guardThen` (`useAppStore.ts`) calls `api.scanStaged`. Any finding opens
   `SecretGuardDialog` instead of writing. **Both commit and amend route through
   this one function** — see below.
2. `SecretGuardDialog` (`src/components/SecretGuardDialog.tsx`) offers cancel,
   add-to-`.gitignore`, or a typed override — the literal phrase `commit anyway`
   (`SecretGuardDialog.tsx:7`). Only the override calls `confirmOverrideCommit`
   with `allowSecrets = true`, which dispatches on `secretGuardAction` so the
   override resumes the write the user actually asked for.
3. `secret_scan::gate` (`secret_scan/mod.rs`) **re-scans** unless `allow_secrets`.
   This is the enforcement point; the frontend pre-scan is only for the rich
   dialog. A caller that skips step 1 is still blocked.

`gate` is one shared function rather than a block copied into each command,
because the copies drifted: `amend_commit` shipped with **no gate at all**, so
staging a key and pressing "Amend last commit" — the button beside Save, in the
same panel — committed it with no scan and no dialog. Amending an unpushed commit
is the normal case, not an edge one, and `push` correctly has no gate of its own
because it trusts this one. Any future command that turns the staged tree into a
commit must call `gate`; the tests in `secret_scan/mod.rs` cover block, override,
and clean-passthrough.

`git::ops::commit` (`ops.rs:441`) then validates in order: non-empty message,
resolvable identity (`ops.rs:536`), and something actually staged — the last
detected by comparing the written tree oid to the parent's (`ops.rs:463`), or
tree emptiness on an initial commit.

A `no_identity` failure is recoverable, so `promptForIdentity`
(`useAppStore.ts:371`) intercepts it, closes the secret guard, and opens
`IdentityDialog`; `saveIdentityAndCommit` (`useAppStore.ts:189`) writes the
identity to **global** git config and resumes the same commit, carrying the
original `allowSecrets` forward so the override isn't silently dropped.

Discard is routed through `DiscardDialog` via `discardTarget`
(`useAppStore.ts:303-323`) because `discard_paths` (`ops.rs:350`) does a
`reset_default` followed by a **force** checkout — it deliberately destroys
working-tree edits. Untracked files are untouched (no HEAD version to restore).

Amend (`ops.rs:381`) refuses when the commit is reachable from the upstream ref
(`commit_is_pushed`, `ops.rs:421`). That check reads the last-fetched
remote-tracking ref, so it can only be stale in the safe direction.

### Sync: fetch / pull / push

All three resolve credentials through `credentials_cb` (`git/sync.rs:21`), in
order: stored GitHub token (only when `is_github_token_url` says the URL is
really `https://github.com`), then the user's credential helper, then the SSH
agent, then `Cred::default()`.

**pull** (`sync.rs:128`) resolves the upstream (`sync.rs:95`, falling back to
`origin` when tracking was never configured), fetches the branch plus all tags,
and refuses anything that isn't a fast-forward with an explanatory error
(`sync.rs:152`) — there is no merge support. The fast-forward itself has a
deliberate ordering documented at `sync.rs:158-167`: **checkout first, then move
the ref**, using a SAFE checkout so a conflicting local edit aborts with nothing
changed. Tests at `sync.rs:402` and `sync.rs:430` pin both halves of this (refused
pull is a clean no-op; unrelated local edits survive).

**push** (`sync.rs:190`) pushes `refs/heads/<b>:refs/heads/<b>`. libgit2 reports
per-ref rejections through the `push_update_reference` callback rather than as an
`Err`, so the rejection is captured into an `Rc<RefCell<…>>` and checked after
(`sync.rs:196-219`) — without that, a rejected push looks like success.
`push_rejection_error` (`sync.rs:67`) special-cases two messages: the missing
`workflow` OAuth scope (tells the user to sign out and back in) and
non-fast-forward (tells them to pull first). It then calls `set_tracking`
(`ops.rs:228`) to create the remote-tracking ref libgit2 doesn't create itself,
which is what makes ahead/behind work after a first push. Tags are pushed
best-effort afterwards (`sync.rs:236`); a tag rejection appends a soft note rather
than failing a successful branch push.

**fetch** (`sync.rs:259`) is the only non-destructive way to learn you're behind.
Remote selection (`fetch_remote_name`, `sync.rs:312`) prefers the branch's
upstream, then `origin`, then the sole configured remote.
`fetch_and_track_origin` (`sync.rs:288`) is separate and fetches `origin`
*explicitly* — the comment at `sync.rs:290-292` explains why it must not reuse the
preference logic.

### GitHub auth

Device flow. `start_login` (`auth.rs:105`) posts to
`github.com/login/device/code` with scopes `repo read:user workflow`
(`auth.rs:18`); `workflow` is required because GitGlass writes files under
`.github/workflows` and GitHub rejects the entire push without it.
`poll_login` (`auth.rs:136`) is a **single** poll — the loop lives in the
frontend (`useGithubStore.ts:160-197`), guarded by a module-level `pollToken`
(`useGithubStore.ts:68`) so a cancelled or restarted sign-in cannot keep polling,
and bounded by the `expires_in` deadline.

The token is written to the OS keychain only, service `GitGlass` / account
`github-token` (`auth.rs:8-9`). `current_status` (`auth.rs:190`) distinguishes a
*rejected* token (401/403 → genuinely signed out) from a *transient* failure
(network/5xx → stay signed in, show the last-known identity from the `last_good`
cache at `auth.rs:179`). That's why a flaky connection doesn't flip the UI to
signed-out while a valid token is still held.

### Deciding what is really GitHub

`src-tauri/src/github/url.rs` is new on this branch and is a security boundary,
not a helper. It answers: may this URL receive a token carrying `repo` +
`workflow` scope?

- `is_github_token_url` (`url.rs:22`) requires scheme `https` **and** parsed host
  `== github.com` (case-insensitive). `http://` is refused because the token
  travels as an HTTP Basic password.
- `token_credentials` (`url.rs:36`) re-checks the host **on every callback
  invocation**, because libgit2 follows redirects and calls back with the URL it
  actually reached (`url.rs:31-35`).
- Git transport uses Basic auth with username `x-access-token` and the token as
  the password — not the REST `Bearer` header, and not the token as username,
  which GitHub rejects for OAuth tokens (`url.rs:45-49`).
- `parse_owner_repo` (`url.rs:63`) accepts the three forms git writes: `https://`,
  `ssh://`, and scp-like `git@github.com:owner/repo.git` (handled separately at
  `url.rs:81` since it isn't a parseable URL). `split_owner_repo` (`url.rs:94`)
  rejects extra path segments so a browser URL like `/owner/repo/tree/main` can't
  yield a repo name of `repo/tree/main`, and strips exactly one `.git` suffix so
  `foo.git.git` survives.

The test at `url.rs:229` drives a real libgit2 fetch against a local server that
demands Basic auth and asserts the token never appears in the received bytes.
`rejects_look_alike_hosts` (`url.rs:120`) enumerates the five URLs that defeat a
`contains("github.com")` check.

The frontend has a parallel `isGithub` (`src/store/useGithubStore.ts:356`, tested
in `src/store/isGithub.test.ts`) using the same host comparison. It gates only
which buttons appear; the Rust side is the enforcement.

### Publish

`github::remote::publish` (`remote.rs:259`) is the longest path:

1. `require_token` (`auth.rs:226`).
2. Fetch the GitHub user to build a fallback committer identity
   (`login` / `login@users.noreply.github.com`) for users with no git config
   (`remote.rs:270-278`).
3. `ensure_repo_with_commit` (`remote.rs:172`) on a blocking thread. It records
   whether `.git` was created here and **removes it on failure** so a failed
   publish leaves no half-made repo (`remote.rs:180-187`).
4. `ensure_repo_inner` (`remote.rs:190`) first rejects embedded repos by name
   (`find_embedded_repos`, `ops.rs:102`) — git cannot nest repos and the raw error
   is unreadable. **If the repo already has commits it returns immediately**
   (`remote.rs:217`); the staging + secret scan at `remote.rs:223-239` runs only
   for an unborn branch. See Drift.
5. `api::create_repo` (`api.rs:122`, `auto_init: false`).
6. `set_origin_and_push` (`remote.rs:72`) on a blocking thread — same
   rejection-capture pattern as `sync::push`, sharing `push_rejection_error`
   (`remote.rs:119`), then `set_tracking`.

`set_origin` (`remote.rs:134`) is the different, non-creating path: it validates
the URL as GitHub via `parse_owner_repo` (`remote.rs:150`) so `origin` never gets
wired to junk, then best-effort fetches and establishes tracking so ahead/behind
works immediately instead of staying blank until the first push.

`clone` (`remote.rs:307`) offers the token *only* when the URL is GitHub
(`remote.rs:308`); a non-GitHub clone is still allowed but proceeds
unauthenticated.

All blocking git2 work runs through `run_blocking` (`remote.rs:417`,
`spawn_blocking`) so the async runtime isn't stalled.

### Secret scanning

Two entry points with different sources:

- `scan_staged` diffs HEAD tree → index with `context_lines(0)` and examines
  **only `+` lines**. It backs `secret_scan::gate` — used by `commit`,
  `amend_commit`, `init_repo` (`ops.rs:187`), and the first-commit branch of
  publish.
- `scan_folder` walks the filesystem for a pre-publish dry run and for the
  existing-history branch of publish. Respects `.gitignore` when the folder is a
  repo, skips `SCAN_SKIP_DIRS`, files > 1 MB, binaries (NUL-byte test), and caps
  at 3,000 files.

`SCAN_SKIP_DIRS` holds only *generated output* — dependency trees, build
artifacts, virtualenvs, VCS internals — and is shared with the file watcher,
which asks the same question. It deliberately does **not** list `.github`,
`.vscode`, `.idea` or `.terraform`: those hold hand-written config that really
can contain credentials. `scan_folder` previously skipped every directory
starting with `.`, which meant a folder carrying `.aws/credentials`, `.ssh/id_rsa`
or `.config/gh/hosts.yml` scanned clean and the publish dialog reported it safe.
Tests in `secret_scan/mod.rs` pin both directions.

Detection (`scan_line`, `mod.rs:258`) is three layers: eight provider-prefix
regexes (`mod.rs:41`), a PEM private-key header (`mod.rs:90`), and a generic
`KEY = value` assignment rule (`mod.rs:99`) gated on entropy ≥ 3.5 bits/char and
"tokenish" content, or unconditionally in a real `.env` file. Comment lines are
skipped entirely (`mod.rs:260`). Two notable carve-outs found in the code:
`value.starts_with(':')` suppresses false hits on `Foo::BAR` namespace paths that
the lookbehind-less regex crate can't exclude (`mod.rs:297-303`), and
`is_dotenv` (`mod.rs:331`) excludes `.env.example|sample|template|dist`.
`is_allowlisted` (`mod.rs:345`) filters placeholders, `$VAR`/`%VAR%`/`os.environ`/
`process.env`/`secrets.` references, SRI hashes, and single-repeated-char runs.
Every `preview` is redacted (`Finding.preview`, `mod.rs:32`) so a finding is never
a new place the secret leaks.

### Rendering the two long lists

Both lists in this app are unbounded in the user's data, and both sit downstream
of the watcher, so a refresh can re-render them at watcher frequency.

- **`FileList`** virtualizes with `@tanstack/react-virtual` — only visible rows
  are mounted. `FileRow` is additionally `memo`'d with a field comparator rather
  than reference equality, because every refresh deserializes fresh entry objects
  over IPC and reference equality would never hit. `modifiedMs` is deliberately
  excluded from the comparison: nothing renders it, so a touched mtime shouldn't
  cost a re-render.
- **`DiffViewer`** is mounted unconditionally by `App.tsx`, so closing it does not
  unmount it. Its load effect explicitly clears `diff` when `diffPath` goes null;
  without that the last-viewed `FileDiff` stayed in the webview for the rest of
  the session. Line highlighting is memoized per line — it previously re-ran for
  every line on every render, including the unified/split toggle, which
  re-highlighted the whole file synchronously. The backend caps a diff at
  `MAX_DIFF_LINES` (20k, `git/diff.rs`) and sets `truncated`, which the viewer
  surfaces rather than silently showing a partial diff.

## Data

Nothing is persisted server-side and there is no local database.

| What | Where | Shape |
|---|---|---|
| GitHub token | OS keychain, `GitGlass`/`github-token` | opaque string (`auth.rs:8`) |
| Git identity | user's **global** `~/.gitconfig` | `user.name`, `user.email` (`ops.rs:39,78`) |
| UI prefs | `localStorage` key `gitglass.ui` | `{theme, pinned, recent}` only (`useAppStore.ts:353-357`) |
| Everything else | derived per call from libgit2 / GitHub API | never cached to disk |

DTOs crossing IPC are all `#[serde(rename_all = "camelCase")]` and mirrored in
`src/lib/types.ts`: `DirListingDto`/`RepoInfoDto`/`DirEntryDto` (`fs/mod.rs:8-43`),
`FileDiffDto` (`diff.rs:29`), `BranchInfoDto` (`branch.rs:12`), `Finding`
(`secret_scan/mod.rs:23`), `Identity` (`ops.rs:12`), `AuthStatus`/`DeviceCodeDto`/
`LoginPoll` (`auth.rs:68-102`), `PublishResult`/`PrResult` (`remote.rs:19,25`),
and the `api.rs` GitHub types.

The `last_good` auth cache (`auth.rs:179`) is process-global mutable state — the
only such state in the backend besides `WatchState`.

## Decisions

**libgit2, never the git CLI** (`Cargo.toml:31`). Stated reasons: deterministic
behavior independent of a system git, structured errors, no shell-injection
surface. `vendored-libgit2` means end users install nothing.
`vendored-openssl` has a specific documented reason (`Cargo.toml:24-29`): the
macOS universal build cross-compiles the Intel half on an Apple Silicon runner
that has no x86_64 OpenSSL.

**Compute status once per listing** (`status.rs:41`, `fs/mod.rs:56`). Keeps
navigation instant on large directories and leaves headroom inside the scanner's
2 s budget.

**Global, not per-repo, git identity** (`ops.rs:32-38`). "Who you are" isn't a
property of one project; a first-time user who fixed it per-repo would hit the
wall again in the next one.

**Backend re-scan as the secret gate, frontend scan for UX** (`lib.rs:83-89`).
The UI pre-scan exists to render a rich dialog; the command enforces
independently, so there is no bypass for a caller that skips it.

**Host comparison, not substring, for token release** (`url.rs:1-8`). The file's
own header states this is a security boundary; the reason is enumerated in the
`rejects_look_alike_hosts` test.

**Checkout-before-ref-move on fast-forward pull** (`sync.rs:158-167`). Reason
given in-code: the checkout's baseline must be the old tree so it both applies
incoming changes and detects conflicting local edits, and a refused SAFE checkout
leaves nothing to roll back.

**Delete-branch protection** (`branch.rs:96`). The comment records that GitGlass
previously always force-deleted, silently orphaning commits; `branch_is_merged`
(`branch.rs:122`) is deliberately conservative — any uncertainty reads as
unmerged.

**Zero telemetry.** Asserted in `docs/SECURITY.md`. I found no analytics or
crash-reporting code in `src/` or `src-tauri/src/`; outbound requests are
`api.github.com` and `github.com` (`api.rs:8`, `auth.rs:109,139`) plus the
configured updater endpoint. Consistent with the claim as far as this pass looked.

**Why the frontend drives the device-flow poll loop rather than the backend** —
no reason found in code, comments, or git history (the repo has a single commit,
`c0e331d`, so `git log`/`git blame` yield nothing). Unknown.

## Drift

Ordered by how much a wrong assumption here would cost an auditor.

> Items 1, 2 and 3 were **fixed** on branch `m5.1-sync-hardening` after a
> security and runtime audit. They are kept here with their resolution because
> the shape of each defect is worth knowing when reading the surrounding code.
> Item 7 is **still open** — only its documentation improved. Everything else in
> this section is still open.

1. ~~**`reveal_in_os` does nothing.**~~ **Fixed.** It now calls
   `tauri_plugin_opener::reveal_item_in_dir` (`lib.rs:41`). Previously it only
   checked that the path existed and returned `Ok(())`, so the "Reveal in File
   Explorer" context item (`FileRow.tsx`) silently no-oped — and because the UI
   only surfaces the error case, a no-op was indistinguishable from success.

2. ~~**Publish does not secret-scan a repo that already has commits.**~~
   **Fixed on both sides.** `ensure_repo_inner` now runs `scan_folder` on the
   working tree before its early return (`remote.rs:216`), because `scan_staged`
   sees an empty diff on that path; covered by
   `ensure_repo_blocks_secrets_when_the_repo_already_has_commits`. The frontend
   pre-publish scan now runs automatically when the dialog opens rather than
   waiting for a button, and Publish stays disabled until a scan has completed
   (`GithubDialogs.tsx`) — previously `hasSecrets` stayed false until the user
   chose to look, so the disable condition was satisfied by never scanning.
   **Still true, and stated in the dialog:** the scan covers the working tree,
   not history. A secret committed and later deleted is still in the objects
   being pushed.

3. ~~**`watcher.rs::is_relevant` doesn't do what its comment says.**~~
   **Fixed.** It now matches `SCAN_SKIP_DIRS` (shared with the secret scanner)
   component-wise, plus `.git`. This is load-bearing for performance, not tidiness
   — see [Watching](#watching).

4. **The Cargo.toml comment on `reqwest` is wrong.** `Cargo.toml:47` says
   "native-tls = SChannel on Windows". The declared features are `["json"]` only,
   and `Cargo.lock:3238-3263` resolves `reqwest` 0.13.4 with `hyper-rustls`,
   `rustls`, and `rustls-platform-verifier` — i.e. **rustls**, not SChannel. Any
   audit of TLS behavior should target rustls.

5. **Two independent implementations of the same token-host check.**
   `git/sync.rs:29` calls `is_github_token_url` inline inside its own
   `credentials_cb` rather than using `url::token_credentials` (`url.rs:36`).
   Both are correct today and both re-check per invocation, but only
   `token_credentials` is covered by the end-to-end "token never reaches a
   non-GitHub server" test (`url.rs:229`) — and `sync.rs` is the path that
   pull/push/fetch actually use. A future change to one won't be caught by the
   other's tests.

6. **`docs/SECURITY.md` principle 1 is stale in its parenthetical.** It says
   keychain storage is "(Wired in M4; the boundary is designed for it now.)" —
   it is fully implemented (`auth.rs:22-41`). The later "GitHub token storage
   (M4 — implemented)" section is accurate. Same doc, contradicting itself.

7. **Updater has a placeholder public key — still open, now documented.**
   `pubkey` is the literal `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY`. It fails
   *closed* (the plugin base64-decodes `pubkey` before installing), so there is
   no forged-update path; the consequence is that a shipped build can never be
   patched. Two further gaps make the channel dead regardless:
   `bundle.createUpdaterArtifacts` is unset, so no `.sig`/`latest.json` is ever
   produced, and nothing in `src/` imports the updater plugin, so it is never
   invoked. See `docs/SIGNING.md` for the full checklist.

   **Note the dead `active` key.** An earlier pass set `"active": false` here
   believing it disabled the updater. It did nothing: `active` is a Tauri **v1**
   field, and v2's updater `Config` has no such member — the plugin's
   `Deserialize` impl ignores unknown keys. The key has been removed rather than
   left as a no-op that reads like a control.

8. **The previous version of this document described M1 only.** It listed
   `stage.rs`, `commit.rs`, `remote.rs`, `diff.rs`, `branch.rs`, `secret_scan/`,
   and `github/` as future "extension points". All exist; the module split landed
   differently (`git/ops.rs` absorbed stage/commit/discard/amend/ignore/identity;
   `github/remote.rs` rather than `git/remote.rs`; no `credentials` module — the
   keychain wrapper lives in `github/auth.rs:22-41`).

Minor, non-structural:

- `useGithubStore.loadStatus` (`useGithubStore.ts:97`) has `try`/`finally` with no
  `catch`; a rejection propagates as an unhandled rejection. Currently
  unreachable since `github_status` always returns `Ok` (`lib.rs:243`), but that
  coupling is implicit.
- ~~`watcher.rs` defines a dead `Roots` type behind `#[allow(dead_code)]`.~~
  Removed along with the `FileIdMap` it was speculating about.
- `ErrorKind::Unknown` (`error.rs:39`) is `#[allow(dead_code)]` in Rust but *is*
  produced — by `run_blocking` (`remote.rs:423`) and by the TS normalizer
  (`tauri.ts:41`).

## Not covered

Read fully: `lib.rs`, `error.rs`, `fs/mod.rs`, `watcher.rs`, `git/status.rs`,
`git/ops.rs`, `git/sync.rs`, `git/branch.rs`, `github/auth.rs`,
`github/remote.rs`, `github/url.rs`, `secret_scan/mod.rs`, `src/lib/tauri.ts`,
`src/store/useAppStore.ts`, `src/store/useGithubStore.ts`, `src/App.tsx`,
`src/components/SecretGuardDialog.tsx`, `src/components/SyncControls.tsx`, plus
all manifests and `capabilities/default.json`.

Read partially: `github/api.rs` (through `create_pull`, ~line 200 of 565 — the
list/workflow/tag endpoints below that are skimmed only), `git/diff.rs` (DTOs and
signature), `src/components/GithubDialogs.tsx` (publish dialog only).

**Not read at all**, so "not documented" here means "not looked at", not "not
present": `src-tauri/src/automation/mod.rs` (242 lines — the workflow-YAML
generator; only its command signatures are documented), `src/lib/cron.ts`,
`src/lib/highlight.ts`, `src/lib/paths.ts`, `src/lib/types.ts` (inferred from the
Rust DTOs, not verified line by line), and most of `src/components/` —
`AccountAutomations`, `AutomationsButton`, `AutomationsDialog`, `Breadcrumbs`,
`CollapsibleSection`, `CommitPanel`, `ContextMenu`, `DiffViewer`, `DiscardDialog`,
`ErrorState`, `FileList`, `GithubButton`, `GithubSidebar`, `IdentityDialog`,
`InitRepoButton`, `InitRepoDialog`, `MyGithubRepos`, `Sidebar`, `StatusBadge`,
`StatusLegend`, `TitleBar`, `Toaster`, `icons`. `BranchMenu.tsx` and `FileRow.tsx`
were read only in part.

No build or test run was performed as part of this pass; all claims are from
reading source, not from observed runtime behavior.
