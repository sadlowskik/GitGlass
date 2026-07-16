import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  AuthStatus,
  Automation,
  BranchInfo,
  DeviceCode,
  DirListing,
  FileDiff,
  Finding,
  GhItem,
  Identity,
  LoginPoll,
  PrResult,
  PublishResult,
  RepoSummary,
  WorkflowRun,
} from "./types";

/**
 * Thin, typed wrapper around Tauri IPC. Every backend call funnels through
 * `call()` so error normalization lives in exactly one place — the UI never
 * sees a raw libgit2 or io::Error string.
 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw normalizeError(err);
  }
}

function normalizeError(err: unknown): AppError {
  // The backend returns AppError already-shaped; anything else is unexpected.
  if (err && typeof err === "object" && "message" in err && "kind" in err) {
    return err as AppError;
  }
  return {
    kind: "unknown",
    message: "Something went wrong. Please try again.",
    detail: typeof err === "string" ? err : JSON.stringify(err),
  };
}

export const api = {
  /** List a directory, annotated with git status when inside a repo. */
  listDir: (path: string) => call<DirListing>("list_dir", { path }),

  /** Resolve the user's home directory as a sensible default root. */
  homeDir: () => call<string>("home_dir"),

  /** Reveal a path in the OS file manager (Explorer/Finder). */
  revealInOs: (path: string) => call<void>("reveal_in_os", { path }),

  // --- M2: Git actions ---
  /** Stage the given absolute paths (files or folders). */
  stagePaths: (repo: string, paths: string[]) => call<void>("stage_paths", { repo, paths }),

  /** Unstage the given absolute paths, back to HEAD. */
  unstagePaths: (repo: string, paths: string[]) => call<void>("unstage_paths", { repo, paths }),

  /** Scan the staged changes for secrets (M3). */
  scanStaged: (repo: string) => call<Finding[]>("scan_staged", { repo }),

  /** Dry-run scan of a whole folder before publishing (preview leaks). */
  scanFolder: (path: string) => call<Finding[]>("scan_folder", { path }),

  /**
   * Commit the staged index; resolves to the new commit id. `allowSecrets` must
   * only be set after the user completes the typed override — the backend
   * re-scans and blocks otherwise.
   */
  commit: (repo: string, message: string, allowSecrets: boolean) =>
    call<string>("commit", { repo, message, allowSecrets }),

  /** Append paths to .gitignore and unstage them (secret dialog action). */
  ignorePaths: (repo: string, paths: string[]) => call<void>("ignore_paths", { repo, paths }),

  /** The user's configured Git identity (may be unset). */
  gitIdentity: () => call<Identity>("git_identity"),

  /** Turn a plain folder into a local Git repo + first commit. No GitHub. */
  initRepo: (
    path: string,
    name: string | null,
    email: string | null,
    allowSecrets: boolean,
  ) => call<void>("init_repo", { path, name, email, allowSecrets }),

  /** Fetch + fast-forward the current branch; resolves to a friendly summary. */
  pull: (repo: string) => call<string>("pull", { repo }),

  /** Push the current branch to upstream; resolves to a friendly summary. */
  push: (repo: string) => call<string>("push", { repo }),

  // --- M5: Diffs & branches ---
  fileDiff: (repo: string, path: string) => call<FileDiff>("file_diff", { repo, path }),
  listBranches: (repo: string) => call<BranchInfo[]>("list_branches", { repo }),
  createBranch: (repo: string, name: string, checkout: boolean) =>
    call<void>("create_branch", { repo, name, checkout }),
  switchBranch: (repo: string, name: string) => call<void>("switch_branch", { repo, name }),
  deleteBranch: (repo: string, name: string) => call<void>("delete_branch", { repo, name }),

  // --- Automations: scheduled Python workflows ---
  listPythonFiles: (repo: string) => call<string[]>("list_python_files", { repo }),
  listWorkflows: (repo: string) => call<{ file: string }[]>("list_workflows", { repo }),
  hasRequirements: (repo: string) => call<boolean>("automation_context", { repo }),
  createWorkflow: (args: {
    repo: string;
    name: string;
    pythonPath: string;
    cron: string;
    pythonVersion: string;
    installRequirements: boolean;
  }) => call<string>("create_workflow", args),

  // --- M4: GitHub ---
  githubStatus: () => call<AuthStatus>("github_status"),
  githubStartLogin: (clientId: string) =>
    call<DeviceCode>("github_start_login", { clientId }),
  githubPollLogin: (clientId: string, deviceCode: string) =>
    call<LoginPoll>("github_poll_login", { clientId, deviceCode }),
  githubSignOut: () => call<void>("github_sign_out"),
  githubPublish: (
    repo: string,
    name: string,
    priv_: boolean,
    description: string | null,
    allowSecrets: boolean,
  ) =>
    call<PublishResult>("github_publish", {
      repo,
      name,
      private: priv_,
      description,
      allowSecrets,
    }),
  githubClone: (url: string, dest: string) => call<string>("github_clone", { url, dest }),
  githubOpenPr: (repo: string, title: string, body: string) =>
    call<PrResult>("github_open_pr", { repo, title, body }),
  githubListPrs: (repo: string) => call<GhItem[]>("github_list_prs", { repo }),
  githubListIssues: (repo: string) => call<GhItem[]>("github_list_issues", { repo }),
  githubMyRepos: () => call<RepoSummary[]>("github_my_repos"),
  githubWorkflowRuns: (repo: string) => call<WorkflowRun[]>("github_workflow_runs", { repo }),
  githubAutomations: () => call<Automation[]>("github_automations"),
  githubRunNow: (repoFullName: string, workflowId: number, gitRef: string) =>
    call<void>("github_run_now", { repoFullName, workflowId, gitRef }),
  githubCreateRelease: (repo: string, tag: string) =>
    call<string>("github_create_release", { repo, tag }),
};

/**
 * Subscribe to backend file-watch events for a repo. The backend debounces
 * and emits "fs:changed" with the affected repo root; the UI refreshes the
 * current listing if it is inside that root.
 */
export function onFsChanged(handler: (repoRoot: string) => void): Promise<UnlistenFn> {
  return listen<string>("fs:changed", (e) => handler(e.payload));
}
