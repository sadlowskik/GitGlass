// These types mirror the serde structs returned by the Rust backend
// (see src-tauri/src/fs/mod.rs and src-tauri/src/git/status.rs).
// Keep them in sync — the CI `typecheck` job guards the frontend side only.

export type GitStatus =
  | "staged"
  | "modified"
  | "untracked"
  | "ignored"
  | "conflict"
  | "clean";

/** Rolled-up status for a folder: the "worst"/most-actionable child state. */
export interface DirEntry {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  sizeBytes: number;
  modifiedMs: number | null;
  /** null when the entry lives outside any git repo. */
  gitStatus: GitStatus | null;
  /** True for a folder whose descendants include a non-clean, non-ignored change. */
  hasChanges: boolean;
}

export interface RepoInfo {
  /** Absolute path to the working directory root. */
  root: string;
  /** Current branch shorthand, or null on a detached HEAD. */
  branch: string | null;
  /** Commits ahead/behind upstream; null when there is no tracking branch. */
  ahead: number | null;
  behind: number | null;
  isDetached: boolean;
  /** Repo-wide counts for the commit panel & sync toolbar. */
  stagedCount: number;
  unstagedCount: number;
  /** `origin` remote URL, if any. */
  remoteUrl: string | null;
}

// --- M4: GitHub ---

export interface AuthStatus {
  connected: boolean;
  login: string | null;
  avatarUrl: string | null;
}

export interface DeviceCode {
  deviceCode: string;
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
}

export interface LoginPoll {
  status: "authorized" | "pending" | "slow_down" | "denied" | "expired" | "error";
  auth: AuthStatus | null;
  interval: number | null;
}

export interface PublishResult {
  htmlUrl: string;
}

export interface PrResult {
  htmlUrl: string;
  number: number;
}

/** Open PR or issue for the sidebar list. */
export interface GhItem {
  number: number;
  title: string;
  url: string;
  state: string;
  author: string | null;
}

/** One of the signed-in user's repositories (account-wide sidebar list). */
export interface RepoSummary {
  name: string;
  fullName: string;
  htmlUrl: string;
  cloneUrl: string;
  defaultBranch: string;
  private: boolean;
}

/** A workflow (automation) somewhere in the user's account. */
export interface Automation {
  repoFullName: string;
  repoName: string;
  name: string;
  id: number;
  htmlUrl: string;
  defaultBranch: string;
}

/** A GitHub Actions run — "did my automation run?" status. */
export interface WorkflowRun {
  name: string;
  status: string; // queued | in_progress | completed | unknown
  conclusion: string | null; // success | failure | cancelled | ...
  url: string;
}

export interface DirListing {
  path: string;
  parent: string | null;
  /** Repo the path belongs to, or null if not inside a git repo. */
  repo: RepoInfo | null;
  entries: DirEntry[];
}

/** Structured error from the backend — never a raw git/libgit2 string in the UI. */
export interface AppError {
  kind: string;
  /** User-facing, already-friendly message. */
  message: string;
  /** Optional developer detail, shown only in a "details" disclosure. */
  detail: string | null;
}

export interface PinnedRepo {
  path: string;
  name: string;
  pinnedAtMs: number;
}

// --- M5: Diffs & branches ---

export interface DiffLine {
  origin: "context" | "add" | "delete";
  oldLineno: number | null;
  newLineno: number | null;
  content: string;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  isBinary: boolean;
  hunks: DiffHunk[];
}

export interface BranchInfo {
  name: string;
  isCurrent: boolean;
  upstream: string | null;
}

/** The name/email Git stamps on commits; either may be unset for new users. */
export interface Identity {
  name: string | null;
  email: string | null;
}

export type Severity = "high" | "medium";

/** A suspected secret found in the staged diff (M3). `preview` is redacted. */
export interface Finding {
  path: string;
  line: number;
  rule: string;
  severity: Severity;
  preview: string;
}
