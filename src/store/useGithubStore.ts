import { create } from "zustand";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "@/lib/tauri";
import type {
  AppError,
  AuthStatus,
  Automation,
  DeviceCode,
  GhItem,
  RepoSummary,
  WorkflowRun,
} from "@/lib/types";
import { useAppStore } from "./useAppStore";

const CLIENT_ID = (import.meta.env.VITE_GITHUB_CLIENT_ID as string | undefined) ?? "";

type Dialog = null | "signin" | "publish" | "clone" | "connect" | "pr" | "release";

interface GithubState {
  status: AuthStatus;
  loadingStatus: boolean;

  dialog: Dialog;
  openDialog: (d: Dialog) => void;
  closeDialog: () => void;

  // Device-flow sign-in
  device: DeviceCode | null;
  signInError: string | null;

  // Sidebar PR/issue lists for the current GitHub repo
  prs: GhItem[];
  issues: GhItem[];
  runs: WorkflowRun[];
  listsLoading: boolean;

  // Account-wide: the user's own repositories and automations
  myRepos: RepoSummary[];
  automations: Automation[];
  automationsLoading: boolean;

  busy: boolean;

  loadStatus: () => Promise<void>;
  loadMyRepos: () => Promise<void>;
  loadAutomations: () => Promise<void>;
  runNow: (a: Automation) => Promise<void>;
  startSignIn: () => Promise<void>;
  cancelSignIn: () => void;
  signOut: () => Promise<void>;

  publish: (
    name: string,
    isPrivate: boolean,
    description: string,
    allowSecrets: boolean,
  ) => Promise<void>;
  clone: (url: string, dest: string) => Promise<void>;
  connectOrigin: (url: string) => Promise<void>;
  openPr: (title: string, body: string) => Promise<void>;
  createRelease: (tag: string) => Promise<void>;
  refreshLists: () => Promise<void>;

  open: (url: string) => Promise<void>;
}

// Guards a poll loop so a cancelled/restarted sign-in doesn't keep polling.
let pollToken = 0;

const notify = (t: Parameters<ReturnType<typeof useAppStore.getState>["notify"]>[0]) =>
  useAppStore.getState().notify(t);

const errText = (e: unknown) => (e as AppError)?.message ?? "Something went wrong.";

export const useGithubStore = create<GithubState>((set, get) => ({
  status: { connected: false, login: null, avatarUrl: null },
  loadingStatus: false,

  dialog: null,
  openDialog: (d) => set({ dialog: d, signInError: null }),
  closeDialog: () => {
    pollToken++; // stop any in-flight poll loop
    set({ dialog: null, device: null, signInError: null });
  },

  device: null,
  signInError: null,
  prs: [],
  issues: [],
  runs: [],
  listsLoading: false,
  myRepos: [],
  automations: [],
  automationsLoading: false,
  busy: false,

  loadStatus: async () => {
    set({ loadingStatus: true });
    try {
      const status = await api.githubStatus();
      set({ status });
      if (status.connected) {
        void get().refreshLists();
        void get().loadMyRepos();
        void get().loadAutomations();
      }
    } finally {
      set({ loadingStatus: false });
    }
  },

  loadMyRepos: async () => {
    if (!get().status.connected) {
      set({ myRepos: [] });
      return;
    }
    try {
      set({ myRepos: await api.githubMyRepos() });
    } catch {
      set({ myRepos: [] });
    }
  },

  loadAutomations: async () => {
    if (!get().status.connected) {
      set({ automations: [] });
      return;
    }
    set({ automationsLoading: true });
    try {
      set({ automations: await api.githubAutomations() });
    } catch {
      set({ automations: [] });
    } finally {
      set({ automationsLoading: false });
    }
  },

  runNow: async (a) => {
    try {
      await api.githubRunNow(a.repoFullName, a.id, a.defaultBranch);
      notify({ kind: "success", title: `Started “${a.name}” — check runs in a moment` });
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    }
  },

  startSignIn: async () => {
    set({ dialog: "signin", signInError: null, device: null });
    try {
      const device = await api.githubStartLogin(CLIENT_ID);
      set({ device });
      // Open GitHub's verification page for the user's convenience.
      void get().open(device.verificationUri);

      const myToken = ++pollToken;
      let intervalMs = Math.max(device.interval, 1) * 1000;
      const deadline = Date.now() + device.expiresIn * 1000;

      const tick = async () => {
        if (myToken !== pollToken) return; // cancelled/superseded
        if (Date.now() > deadline) {
          set({ signInError: "The code expired. Please try again." });
          return;
        }
        try {
          const poll = await api.githubPollLogin(CLIENT_ID, device.deviceCode);
          if (myToken !== pollToken) return;
          switch (poll.status) {
            case "authorized":
              set({ status: poll.auth!, dialog: null, device: null });
              notify({ kind: "success", title: `Signed in as ${poll.auth!.login}` });
              void get().refreshLists();
              void get().loadMyRepos();
              void get().loadAutomations();
              return;
            case "slow_down":
              intervalMs = Math.max((poll.interval ?? device.interval) * 1000, intervalMs + 2000);
              break;
            case "pending":
              break;
            case "denied":
              set({ signInError: "Access was denied on GitHub." });
              return;
            case "expired":
              set({ signInError: "The code expired. Please try again." });
              return;
            default:
              set({ signInError: "GitHub sign-in failed. Please try again." });
              return;
          }
          setTimeout(() => void tick(), intervalMs);
        } catch (e) {
          set({ signInError: errText(e) });
        }
      };
      setTimeout(() => void tick(), intervalMs);
    } catch (e) {
      set({ signInError: errText(e) });
    }
  },

  cancelSignIn: () => get().closeDialog(),

  signOut: async () => {
    try {
      await api.githubSignOut();
      set({
        status: { connected: false, login: null, avatarUrl: null },
        prs: [],
        issues: [],
        runs: [],
        myRepos: [],
        automations: [],
      });
      notify({ kind: "info", title: "Signed out of GitHub" });
    } catch (e) {
      notify({ kind: "error", title: errText(e) });
    }
  },

  publish: async (name, isPrivate, description, allowSecrets) => {
    // Publish the current folder — a repo root if it is one, else the plain
    // folder (the backend initializes git for it).
    const listing = useAppStore.getState().listing;
    const folder = listing?.repo?.root ?? listing?.path;
    if (!folder) return;
    set({ busy: true });
    try {
      const res = await api.githubPublish(folder, name, isPrivate, description || null, allowSecrets);
      set({ dialog: null });
      await useAppStore.getState().refresh();
      void get().refreshLists();
      notify({ kind: "success", title: "Published to GitHub" });
      void get().open(res.htmlUrl);
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    } finally {
      set({ busy: false });
    }
  },

  clone: async (url, dest) => {
    set({ busy: true });
    try {
      const path = await api.githubClone(url, dest);
      set({ dialog: null });
      notify({ kind: "success", title: "Repository cloned" });
      await useAppStore.getState().navigate(path);
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    } finally {
      set({ busy: false });
    }
  },

  connectOrigin: async (url) => {
    const repo = useAppStore.getState().listing?.repo?.root;
    if (!repo) {
      notify({ kind: "error", title: "Open a Git repository first." });
      return;
    }
    set({ busy: true });
    try {
      await api.githubSetOrigin(repo, url);
      set({ dialog: null });
      notify({ kind: "success", title: "Connected to GitHub" });
      await useAppStore.getState().refresh();
      void get().refreshLists();
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    } finally {
      set({ busy: false });
    }
  },

  openPr: async (title, body) => {
    const repo = useAppStore.getState().listing?.repo?.root;
    if (!repo) return;
    set({ busy: true });
    try {
      const res = await api.githubOpenPr(repo, title, body);
      set({ dialog: null });
      notify({ kind: "success", title: `Opened pull request #${res.number}` });
      void get().refreshLists();
      void get().open(res.htmlUrl);
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    } finally {
      set({ busy: false });
    }
  },

  createRelease: async (tag) => {
    const repo = useAppStore.getState().listing?.repo?.root;
    if (!repo) return;
    set({ busy: true });
    try {
      const actionsUrl = await api.githubCreateRelease(repo, tag);
      set({ dialog: null });
      // Honest wording: tagging always succeeds, but a build only happens if the
      // repo has a release workflow. We open Actions so the user can see whether
      // one actually ran, rather than promising installers that may never build.
      notify({
        kind: "success",
        title: `Tagged ${tag} — opening Actions to check for a build`,
      });
      void get().open(actionsUrl);
    } catch (e) {
      notify({ kind: "error", title: errText(e), detail: (e as AppError)?.detail });
    } finally {
      set({ busy: false });
    }
  },

  refreshLists: async () => {
    const repo = useAppStore.getState().listing?.repo;
    if (!repo || !get().status.connected || !isGithub(repo.remoteUrl)) {
      set({ prs: [], issues: [], runs: [] });
      return;
    }
    set({ listsLoading: true });
    try {
      const [prs, issues, runs] = await Promise.all([
        api.githubListPrs(repo.root),
        api.githubListIssues(repo.root),
        api.githubWorkflowRuns(repo.root),
      ]);
      set({ prs, issues, runs });
    } catch {
      // Non-fatal: the sidebar just stays empty.
      set({ prs: [], issues: [], runs: [] });
    } finally {
      set({ listsLoading: false });
    }
  },

  open: async (url) => {
    try {
      await openUrl(url);
    } catch {
      // Ignore — worst case the link doesn't open.
    }
  },
}));

/**
 * Whether a remote URL really points at github.com.
 *
 * Substring matching says yes to `https://github.com@evil.example/x` and
 * `https://evil.example/github.com/x`, so compare the parsed host instead.
 * This only gates which buttons appear — the Rust side independently refuses
 * to hand the token to a non-GitHub host — but the two checks must agree, or
 * the UI offers GitHub actions the backend will reject.
 */
export function isGithub(url: string | null | undefined): boolean {
  if (!url) return false;
  const trimmed = url.trim();
  // scp-like syntax (`git@github.com:owner/repo.git`) isn't a parseable URL.
  const scp = /^[^/]*@([^:/]+):/.exec(trimmed);
  if (scp) return scp[1].toLowerCase() === "github.com";
  try {
    return new URL(trimmed).hostname.toLowerCase() === "github.com";
  } catch {
    return false;
  }
}
