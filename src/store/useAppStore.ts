import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, onFsChanged } from "@/lib/tauri";
import type { AppError, DirListing, Finding, PinnedRepo } from "@/lib/types";

type Theme = "dark" | "light";

/** A write that turns the staged tree into a commit. Both need the secret gate. */
type WriteAction = "commit" | "amend";

export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  title: string;
  detail?: string | null;
}

interface AppState {
  theme: Theme;
  toggleTheme: () => void;

  // Navigation
  listing: DirListing | null;
  loading: boolean;
  error: AppError | null;
  navigate: (path: string) => Promise<void>;
  goUp: () => Promise<void>;
  refresh: () => Promise<void>;

  // Sidebar
  pinned: PinnedRepo[];
  recent: string[];
  pin: (repo: PinnedRepo) => void;
  unpin: (path: string) => void;

  // Toasts (transient action feedback)
  toasts: Toast[];
  notify: (t: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;

  // Git actions (M2)
  busy: boolean;
  commitOpen: boolean;
  setCommitOpen: (open: boolean) => void;
  commitMessage: string;
  setCommitMessage: (m: string) => void;
  stage: (paths: string[]) => Promise<void>;
  unstage: (paths: string[]) => Promise<void>;
  pull: () => Promise<void>;
  push: () => Promise<void>;
  fetch: () => Promise<void>;
  amend: () => Promise<void>;

  // Discard uncommitted changes — routed through a confirm dialog (destructive).
  discardTarget: string[] | null;
  requestDiscard: (paths: string[]) => void;
  cancelDiscard: () => void;
  confirmDiscard: () => Promise<void>;

  // Diff viewer (M5)
  diffPath: string | null;
  openDiff: (path: string) => void;
  closeDiff: () => void;

  // Automations panel
  automationsOpen: boolean;
  setAutomationsOpen: (open: boolean) => void;

  // "Make it a repo" (local git init) panel
  initRepoOpen: boolean;
  setInitRepoOpen: (open: boolean) => void;

  // Identity prompt: shown when a commit fails because Git has no name/email.
  // Holds the pending write's allowSecrets and which action to resume.
  identityPrompt: { allowSecrets: boolean; action: WriteAction } | null;
  cancelIdentityPrompt: () => void;
  saveIdentityAndCommit: (name: string, email: string) => Promise<void>;

  // Secret guard (M3)
  secretFindings: Finding[] | null;
  /**
   * Which write the guard is currently gating. Amend goes through the same gate
   * as commit — it writes the staged tree into a commit just as commit does.
   */
  secretGuardAction: WriteAction;
  requestCommit: () => Promise<void>;
  confirmOverrideCommit: () => Promise<void>;
  cancelSecretGuard: () => void;
  ignoreSecrets: (paths: string[]) => Promise<void>;

  // Lifecycle
  bootstrap: () => Promise<void>;
  _unwatch: (() => void) | null;
}

const RECENT_LIMIT = 8;
let toastSeq = 0;

export const useAppStore = create<AppState>()(
  persist(
    (set, get) => ({
      theme: "dark",
      toggleTheme: () => {
        const next: Theme = get().theme === "dark" ? "light" : "dark";
        applyTheme(next);
        set({ theme: next });
      },

      listing: null,
      loading: false,
      error: null,

      navigate: async (path: string) => {
        set({ loading: true, error: null });
        try {
          const listing = await api.listDir(path);
          set({ listing, loading: false });
          rememberRecent(set, get, listing);
          rewatch(set, get, listing);
        } catch (e) {
          set({ error: e as AppError, loading: false });
        }
      },

      goUp: async () => {
        const parent = get().listing?.parent;
        if (parent) await get().navigate(parent);
      },

      refresh: async () => {
        const cur = get().listing?.path;
        if (cur) await get().navigate(cur);
      },

      pinned: [],
      recent: [],
      pin: (repo) =>
        set((s) =>
          s.pinned.some((p) => p.path === repo.path) ? s : { pinned: [...s.pinned, repo] },
        ),
      unpin: (path) => set((s) => ({ pinned: s.pinned.filter((p) => p.path !== path) })),

      toasts: [],
      notify: (t) => {
        const id = ++toastSeq;
        set((s) => ({ toasts: [...s.toasts, { ...t, id }] }));
        // Success/info self-dismiss; errors stay until the user closes them.
        if (t.kind !== "error") {
          setTimeout(() => get().dismissToast(id), 4000);
        }
      },
      dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

      busy: false,
      commitOpen: false,
      setCommitOpen: (open) => set({ commitOpen: open }),
      commitMessage: "",
      setCommitMessage: (m) => set({ commitMessage: m }),

      stage: async (paths) => {
        const repo = get().listing?.repo?.root;
        if (!repo || paths.length === 0) return;
        try {
          await api.stagePaths(repo, paths);
          await get().refresh();
        } catch (e) {
          get().notify(errorToast(e));
        }
      },

      unstage: async (paths) => {
        const repo = get().listing?.repo?.root;
        if (!repo || paths.length === 0) return;
        try {
          await api.unstagePaths(repo, paths);
          await get().refresh();
        } catch (e) {
          get().notify(errorToast(e));
        }
      },

      diffPath: null,
      openDiff: (path) => set({ diffPath: path }),
      closeDiff: () => set({ diffPath: null }),

      automationsOpen: false,
      setAutomationsOpen: (open) => set({ automationsOpen: open }),

      initRepoOpen: false,
      setInitRepoOpen: (open) => set({ initRepoOpen: open }),

      identityPrompt: null,
      cancelIdentityPrompt: () => set({ identityPrompt: null }),

      // Save the identity, then finish the commit the user already asked for —
      // they shouldn't have to retype the message and press save twice.
      saveIdentityAndCommit: async (name, email) => {
        const pending = get().identityPrompt;
        if (!pending) return;
        set({ busy: true });
        try {
          await api.setGitIdentity(name, email);
          set({ identityPrompt: null });
          await doWrite(set, get, pending.action, get().commitMessage, pending.allowSecrets);
        } catch (e) {
          // A rejected name/email keeps the dialog open so it can be corrected.
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      secretFindings: null,
      secretGuardAction: "commit",

      // Scan first; if anything is found, open the guard instead of committing.
      requestCommit: async () => {
        const message = get().commitMessage;
        if (message.trim().length === 0) return;
        await guardThen(set, get, "commit", message);
      },

      // Only reachable after the typed confirmation in SecretGuardDialog.
      confirmOverrideCommit: async () => {
        const action = get().secretGuardAction;
        set({ busy: true });
        try {
          await doWrite(set, get, action, get().commitMessage, true);
        } catch (e) {
          if (!promptForIdentity(set, e, true, action)) get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      cancelSecretGuard: () => set({ secretFindings: null }),

      ignoreSecrets: async (paths) => {
        const repo = get().listing?.repo?.root;
        if (!repo) return;
        try {
          await api.ignorePaths(repo, paths);
          set({ secretFindings: null });
          await get().refresh();
          get().notify({ kind: "success", title: "Added to .gitignore and unstaged" });
        } catch (e) {
          get().notify(errorToast(e));
        }
      },

      pull: async () => {
        const repo = get().listing?.repo?.root;
        if (!repo) return;
        set({ busy: true });
        try {
          const summary = await api.pull(repo);
          await get().refresh();
          get().notify({ kind: "success", title: summary });
        } catch (e) {
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      push: async () => {
        const repo = get().listing?.repo?.root;
        if (!repo) return;
        set({ busy: true });
        try {
          const summary = await api.push(repo);
          await get().refresh();
          get().notify({ kind: "success", title: summary });
        } catch (e) {
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      fetch: async () => {
        const repo = get().listing?.repo?.root;
        if (!repo) return;
        set({ busy: true });
        try {
          const summary = await api.fetch(repo);
          // Refresh so the newly-known "behind" count shows up in the toolbar.
          await get().refresh();
          get().notify({ kind: "success", title: summary });
        } catch (e) {
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      // Discard is destructive, so it flows through a confirmation dialog rather
      // than firing on click. `discardTarget` holds the paths awaiting confirm.
      discardTarget: null,
      requestDiscard: (paths) => {
        if (paths.length > 0) set({ discardTarget: paths });
      },
      cancelDiscard: () => set({ discardTarget: null }),
      confirmDiscard: async () => {
        const repo = get().listing?.repo?.root;
        const paths = get().discardTarget;
        if (!repo || !paths) return;
        set({ busy: true });
        try {
          await api.discardPaths(repo, paths);
          set({ discardTarget: null });
          await get().refresh();
          get().notify({ kind: "success", title: "Changes discarded" });
        } catch (e) {
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      // Amend goes through the same secret gate as commit. It sits next to Save
      // in the commit panel and writes the same staged tree, so a bypass here
      // would be a bypass of the whole gate.
      amend: async () => {
        await guardThen(set, get, "amend", get().commitMessage);
      },

      bootstrap: async () => {
        applyTheme(get().theme);
        try {
          const home = await api.homeDir();
          await get().navigate(home);
        } catch (e) {
          set({ error: e as AppError });
        }
      },

      _unwatch: null,
    }),
    {
      name: "gitglass.ui",
      // Only persist user preferences and sidebar — not transient listing state.
      partialize: (s) => ({ theme: s.theme, pinned: s.pinned, recent: s.recent }),
    },
  ),
);

function errorToast(e: unknown): Omit<Toast, "id"> {
  const err = e as AppError;
  return { kind: "error", title: err?.message ?? "Something went wrong.", detail: err?.detail };
}

/**
 * A commit that failed only because Git has no name/email is recoverable, so
 * open the identity dialog instead of showing a toast the user can't act on.
 * Returns whether the error was handled.
 */
function promptForIdentity(
  set: (partial: Partial<AppState>) => void,
  e: unknown,
  allowSecrets: boolean,
  action: WriteAction,
): boolean {
  if ((e as AppError)?.kind !== "no_identity") return false;
  // Close the secret guard as we move on: reaching a commit means the secret
  // gate is already passed (allowSecrets) or was clear. Leaving it open would
  // stack it behind the identity dialog. `allowSecrets` and `action` are carried
  // in identityPrompt so the resumed write still honours the override and still
  // does what the user originally asked for.
  set({ secretFindings: null, identityPrompt: { allowSecrets, action } });
  return true;
}

/**
 * The single entry point for anything that writes the staged tree: scan first,
 * open the guard if anything turns up, otherwise go ahead. Both commit and
 * amend route through here so neither can acquire an ungated path.
 *
 * The backend re-scans regardless — this is the good UX, not the enforcement.
 */
async function guardThen(
  set: (partial: Partial<AppState>) => void,
  get: () => AppState,
  action: WriteAction,
  message: string,
) {
  const repo = get().listing?.repo?.root;
  if (!repo) return;
  set({ busy: true });
  try {
    const findings = await api.scanStaged(repo);
    if (findings.length > 0) {
      set({ secretFindings: findings, secretGuardAction: action });
      return;
    }
    await doWrite(set, get, action, message, false);
  } catch (e) {
    if (!promptForIdentity(set, e, false, action)) get().notify(errorToast(e));
  } finally {
    set({ busy: false });
  }
}

// Shared write path for the normal flow and the secret override. Throws on
// failure so callers surface the error; only touches success state on success.
async function doWrite(
  set: (partial: Partial<AppState>) => void,
  get: () => AppState,
  action: WriteAction,
  message: string,
  allowSecrets: boolean,
) {
  const repo = get().listing?.repo?.root;
  if (!repo) return;
  if (action === "amend") {
    await api.amendCommit(repo, message, allowSecrets);
  } else {
    await api.commit(repo, message, allowSecrets);
  }
  set({ commitMessage: "", commitOpen: false, secretFindings: null });
  await get().refresh();
  get().notify({
    kind: "success",
    title: action === "amend" ? "Commit amended" : "Changes saved",
  });
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  root.classList.toggle("dark", theme === "dark");
  root.classList.toggle("light", theme === "light");
}

function rememberRecent(
  set: (partial: Partial<AppState>) => void,
  get: () => AppState,
  listing: DirListing,
) {
  if (!listing.repo) return;
  const root = listing.repo.root;
  const recent = [root, ...get().recent.filter((r) => r !== root)].slice(0, RECENT_LIMIT);
  set({ recent });
}

// Re-arm the file watcher so it tracks the repo the user is currently browsing.
function rewatch(
  set: (partial: Partial<AppState>) => void,
  get: () => AppState,
  listing: DirListing,
) {
  get()._unwatch?.();
  const repoRoot = listing.repo?.root;
  if (!repoRoot) {
    set({ _unwatch: null });
    return;
  }
  let disposed = false;
  const unlistenPromise = onFsChanged((changedRoot) => {
    if (changedRoot === repoRoot && get().listing?.path) {
      // Refresh silently — no spinner flash on background fs events.
      void get().refresh();
    }
  });
  set({
    _unwatch: () => {
      disposed = true;
      void unlistenPromise.then((un) => un());
    },
  });
  // Guard against a race where the component unmounts before listen resolves.
  void unlistenPromise.then((un) => {
    if (disposed) un();
  });
}
