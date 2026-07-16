import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, onFsChanged } from "@/lib/tauri";
import type { AppError, DirListing, Finding, PinnedRepo } from "@/lib/types";

type Theme = "dark" | "light";

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

  // Secret guard (M3)
  secretFindings: Finding[] | null;
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

      secretFindings: null,

      // Scan first; if anything is found, open the guard instead of committing.
      requestCommit: async () => {
        const repo = get().listing?.repo?.root;
        const message = get().commitMessage;
        if (!repo || message.trim().length === 0) return;
        set({ busy: true });
        try {
          const findings = await api.scanStaged(repo);
          if (findings.length > 0) {
            set({ secretFindings: findings });
            return;
          }
          await doCommit(set, get, message, false);
        } catch (e) {
          get().notify(errorToast(e));
        } finally {
          set({ busy: false });
        }
      },

      // Only reachable after the typed confirmation in SecretGuardDialog.
      confirmOverrideCommit: async () => {
        const message = get().commitMessage;
        set({ busy: true });
        try {
          await doCommit(set, get, message, true);
        } catch (e) {
          get().notify(errorToast(e));
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

// Shared commit path for both the normal flow and the secret override. Throws on
// failure so callers surface the error; only touches success state on success.
async function doCommit(
  set: (partial: Partial<AppState>) => void,
  get: () => AppState,
  message: string,
  allowSecrets: boolean,
) {
  const repo = get().listing?.repo?.root;
  if (!repo) return;
  await api.commit(repo, message, allowSecrets);
  set({ commitMessage: "", commitOpen: false, secretFindings: null });
  await get().refresh();
  get().notify({ kind: "success", title: "Changes saved" });
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
