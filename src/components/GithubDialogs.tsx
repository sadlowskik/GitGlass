import { useCallback, useEffect, useState } from "react";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { useGithubStore, isGithub } from "@/store/useGithubStore";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import type { AppError, Finding } from "@/lib/types";
import { baseName, isWindowsPath } from "@/lib/paths";
import { CheckIcon, GithubIcon, XIcon } from "./icons";

/** Renders whichever GitHub dialog is currently active. */
export function GithubDialogs() {
  const dialog = useGithubStore((s) => s.dialog);
  if (!dialog) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
      {dialog === "signin" && <SignInDialog />}
      {dialog === "publish" && <PublishDialog />}
      {dialog === "clone" && <CloneDialog />}
      {dialog === "connect" && <ConnectDialog />}
      {dialog === "pr" && <OpenPrDialog />}
      {dialog === "release" && <ReleaseDialog />}
    </div>
  );
}

function ReleaseDialog() {
  const createRelease = useGithubStore((s) => s.createRelease);
  const busy = useGithubStore((s) => s.busy);
  const [tag, setTag] = useState("v0.1.0");

  const valid = /^v\d+\.\d+\.\d+/.test(tag.trim());

  return (
    <Shell title="Create a release">
      <div className="space-y-3">
        <p className="text-xs text-content-muted">
          This puts a version tag on the latest commit of your repository’s{" "}
          <span className="font-medium text-content">default branch on GitHub</span> (push your
          work first if you want it included).
        </p>
        <p className="text-xs text-content-muted">
          If your repo has a release workflow, the tag triggers it and installers for{" "}
          <span className="font-medium text-content">Windows (.exe/.msi)</span> and{" "}
          <span className="font-medium text-content">macOS (.dmg)</span> appear on the Releases
          page. We’ll open the Actions tab so you can watch it — if nothing runs, your repo
          doesn’t have that workflow yet.
        </p>
        <div>
          <label className={label}>Version</label>
          <input
            className={clsx(input, "mono", !valid && tag.length > 0 && "border-git-conflict")}
            value={tag}
            onChange={(e) => setTag(e.target.value)}
            placeholder="v1.0.0"
          />
          {!valid && tag.length > 0 && (
            <p className="mt-1 text-xs text-git-conflict">Use the form v1.0.0 (semantic version).</p>
          )}
        </div>
        <div className="flex justify-end pt-1">
          <button
            className={primaryBtn}
            disabled={busy || !valid}
            onClick={() => void createRelease(tag.trim())}
          >
            {busy ? "Tagging…" : "Create release"}
          </button>
        </div>
      </div>
    </Shell>
  );
}

function Shell({ title, children }: { title: string; children: React.ReactNode }) {
  const close = useGithubStore((s) => s.closeDialog);
  return (
    <div className="glass w-full max-w-md animate-slide-up rounded-2xl p-5">
      <div className="mb-4 flex items-center gap-2">
        <GithubIcon className="h-5 w-5 text-content-muted" />
        <h2 className="flex-1 text-lg font-semibold text-content-strong">{title}</h2>
        <button
          onClick={close}
          className="rounded-lg p-1 text-content-faint transition-colors hover:bg-surface-2 hover:text-content"
          aria-label="Close"
        >
          <XIcon className="h-4 w-4" />
        </button>
      </div>
      {children}
    </div>
  );
}

const label = "mb-1 block text-xs font-medium text-content-muted";
const input =
  "w-full select-text rounded-lg border border-white/10 bg-surface-0 px-2.5 py-1.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-accent";
const primaryBtn =
  "rounded-lg bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40";

function SignInDialog() {
  const device = useGithubStore((s) => s.device);
  const error = useGithubStore((s) => s.signInError);
  const open = useGithubStore((s) => s.open);
  const retry = useGithubStore((s) => s.startSignIn);

  return (
    <Shell title="Connect to GitHub">
      {!device && !error && <p className="text-sm text-content-muted">Starting sign-in…</p>}

      {device && !error && (
        <div className="space-y-4">
          <p className="text-sm text-content-muted">
            Enter this code on GitHub to connect your account:
          </p>
          <div className="rounded-xl bg-surface-0 p-4 text-center">
            <div className="mono text-2xl font-bold tracking-[0.3em] text-content-strong">
              {device.userCode}
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button onClick={() => void open(device.verificationUri)} className={primaryBtn}>
              Open GitHub
            </button>
            <span className="flex items-center gap-2 text-xs text-content-faint">
              <span className="h-3 w-3 animate-spin rounded-full border-2 border-content-faint border-t-transparent" />
              Waiting for you to approve…
            </span>
          </div>
          <p className="text-xs text-content-faint">
            After approving on GitHub, this dialog closes automatically.
          </p>
        </div>
      )}

      {error && (
        <div className="space-y-3">
          <p className="text-sm text-git-conflict">{error}</p>
          <button onClick={() => void retry()} className={primaryBtn}>
            Try again
          </button>
        </div>
      )}
    </Shell>
  );
}

function PublishDialog() {
  const folder = useAppStore((s) => s.listing?.repo?.root ?? s.listing?.path);
  const isRepo = useAppStore((s) => !!s.listing?.repo);
  const remoteUrl = useAppStore((s) => s.listing?.repo?.remoteUrl ?? null);
  const publish = useGithubStore((s) => s.publish);
  const busy = useGithubStore((s) => s.busy);
  const notify = useAppStore((s) => s.notify);

  const [name, setName] = useState(folder ? baseName(folder) : "");
  const [isPrivate, setIsPrivate] = useState(true);
  const [description, setDescription] = useState("");

  // Pre-publish secret preview + typed override.
  const [scan, setScan] = useState<Finding[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanFailed, setScanFailed] = useState(false);
  const [overridePhrase, setOverridePhrase] = useState("");

  const runScan = useCallback(async () => {
    if (!folder) return;
    setScanning(true);
    setScan(null);
    setScanFailed(false);
    try {
      setScan(await api.scanFolder(folder));
    } catch (e) {
      setScanFailed(true);
      notify({ kind: "error", title: (e as AppError)?.message ?? "Scan failed" });
    } finally {
      setScanning(false);
    }
  }, [folder, notify]);

  // Scan as soon as the dialog opens. This used to be opt-in behind a button,
  // which meant `hasSecrets` stayed false until the user chose to look — so the
  // Publish button's "disabled when hasSecrets" check was satisfied by simply
  // never scanning, while the dialog claimed publishing was blocked.
  useEffect(() => {
    void runScan();
  }, [runScan]);

  const hasSecrets = !!scan && scan.length > 0;
  // Don't offer Publish until we know what's in the folder. A failed scan is
  // not a clean scan: fall back to letting the user retry rather than treating
  // "we couldn't look" as "nothing there".
  const scanPending = scanning || (!scan && !scanFailed);
  // Publishing creates a NEW repo and repoints `origin` at it. If this repo is
  // already wired to a non-GitHub remote, that link would be silently replaced —
  // warn, and point them at "Connect" if they meant to link the existing repo.
  const willReplaceRemote = !!remoteUrl && !isGithub(remoteUrl);

  return (
    <Shell title="Publish to GitHub">
      <div className="space-y-3">
        {!isRepo && (
          <p className="rounded-lg bg-surface-0/60 p-2.5 text-xs text-content-muted">
            This folder isn’t tracked by Git yet. GitGlass will set it up and make the first commit
            for you, then push it to a new GitHub repository.
          </p>
        )}
        {willReplaceRemote && (
          <p className="rounded-lg border border-git-conflict/30 bg-git-conflict/10 p-2.5 text-xs text-content-muted">
            This folder is already connected to a remote:{" "}
            <span className="mono break-all text-content">{remoteUrl}</span>. Publishing creates a
            new GitHub repo and <span className="text-content">replaces</span> that connection. If
            you meant to link this folder to a repo that already exists, close this and use{" "}
            <span className="font-medium text-content">Connect</span> instead.
          </p>
        )}
        <div>
          <label className={label}>Repository name</label>
          <input className={input} value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div>
          <label className={label}>Description (optional)</label>
          <input
            className={input}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What is this project?"
          />
        </div>
        <label className="flex cursor-pointer items-center gap-2 text-sm text-content">
          <input
            type="checkbox"
            checked={isPrivate}
            onChange={(e) => setIsPrivate(e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
          Keep this repository private
        </label>

        {/* Pre-publish secret check — runs automatically on open. */}
        <div className="rounded-lg border border-white/10 p-2.5">
          <div className="flex items-center gap-2">
            <button
              onClick={() => void runScan()}
              disabled={scanning || !folder}
              className="rounded-lg bg-surface-2 px-2.5 py-1.5 text-xs font-medium text-content transition-colors hover:bg-surface-3 disabled:opacity-50"
            >
              {scanning ? "Scanning…" : "Re-check for secrets"}
            </button>
            <span className="text-xs text-content-faint">
              Every file is scanned before it leaves your computer.
            </span>
          </div>

          {scanFailed && (
            <p className="mt-2 text-xs font-medium text-git-conflict">
              The scan didn’t finish, so we can’t tell you whether this folder is clean. Try again
              before publishing.
            </p>
          )}

          {scan && !hasSecrets && (
            <>
              <p className="mt-2 flex items-center gap-1.5 text-xs font-medium text-git-staged">
                <CheckIcon className="h-3.5 w-3.5" /> No secrets found in your files.
              </p>
              {isRepo && (
                // Publishing an existing repo uploads its whole history. The
                // scan reads the files as they are now, so a secret that was
                // committed and later deleted still goes up. Say so rather than
                // implying a clean bill of health.
                <p className="mt-1 text-xs text-content-faint">
                  This checks your files as they are now. Anything you committed and later removed
                  is still in this project’s history and will be uploaded too.
                </p>
              )}
            </>
          )}

          {hasSecrets && (
            <div className="mt-2">
              <p className="mb-1.5 text-xs font-medium text-git-conflict">
                {scan!.length} possible secret{scan!.length === 1 ? "" : "s"} found — publishing is
                blocked until these are removed or ignored.
              </p>
              <div className="max-h-40 space-y-1 overflow-y-auto rounded-lg bg-surface-0/60 p-1.5">
                {scan!.map((f, i) => (
                  <div key={i} className="flex items-center gap-2 px-1 py-0.5">
                    <span
                      className={clsx(
                        "shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase",
                        f.severity === "high"
                          ? "bg-git-conflict/15 text-git-conflict"
                          : "bg-git-modified/15 text-git-modified",
                      )}
                    >
                      {f.rule}
                    </span>
                    <span className="mono min-w-0 flex-1 truncate text-xs text-content-muted">
                      {f.path}
                      <span className="text-content-faint">:{f.line}</span>
                    </span>
                    <span className="mono shrink-0 text-xs text-content-faint">{f.preview}</span>
                  </div>
                ))}
              </div>

              {/* Typed override — for reviewed false positives (e.g. example
                  keys in source). No one-click bypass. */}
              <details className="group mt-2 rounded-lg border border-white/10 p-2">
                <summary className="cursor-pointer select-none text-[11px] font-medium text-content-muted">
                  These are safe (example/test values) — let me publish anyway
                </summary>
                <div className="mt-2">
                  <p className="mb-1.5 text-[11px] text-content-muted">
                    Only if you’ve reviewed them. Type{" "}
                    <span className="mono font-semibold text-content-strong">publish anyway</span> to
                    confirm.
                  </p>
                  <div className="flex items-center gap-2">
                    <input
                      value={overridePhrase}
                      onChange={(e) => setOverridePhrase(e.target.value)}
                      placeholder="publish anyway"
                      className={clsx(input, "mono flex-1")}
                    />
                    <button
                      onClick={() => void publish(name.trim(), isPrivate, description, true)}
                      disabled={
                        busy ||
                        name.trim().length === 0 ||
                        overridePhrase.trim().toLowerCase() !== "publish anyway"
                      }
                      className={clsx(
                        "shrink-0 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors",
                        overridePhrase.trim().toLowerCase() === "publish anyway" && !busy
                          ? "bg-git-conflict text-white hover:opacity-90"
                          : "cursor-not-allowed bg-surface-2 text-content-faint",
                      )}
                    >
                      {busy ? "Publishing…" : "Publish anyway"}
                    </button>
                  </div>
                </div>
              </details>
            </div>
          )}
        </div>

        <div className="flex justify-end pt-1">
          <button
            className={primaryBtn}
            disabled={busy || scanPending || name.trim().length === 0 || hasSecrets}
            onClick={() => void publish(name.trim(), isPrivate, description, false)}
          >
            {busy ? "Publishing…" : scanPending ? "Checking…" : "Publish"}
          </button>
        </div>
      </div>
    </Shell>
  );
}

function CloneDialog() {
  const clone = useGithubStore((s) => s.clone);
  const busy = useGithubStore((s) => s.busy);
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState<string | null>(null);

  const repoName = deriveName(url);
  const dest = parent && repoName ? joinPath(parent, repoName) : null;

  const chooseFolder = async () => {
    const picked = await openFolderDialog({ directory: true, multiple: false, title: "Clone into" });
    if (typeof picked === "string") setParent(picked);
  };

  return (
    <Shell title="Clone from GitHub">
      <div className="space-y-3">
        <div>
          <label className={label}>Repository URL</label>
          <input
            className={input}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://github.com/owner/repo"
          />
        </div>
        <div>
          <label className={label}>Clone into</label>
          <div className="flex items-center gap-2">
            <button
              onClick={() => void chooseFolder()}
              className="rounded-lg bg-surface-2 px-3 py-1.5 text-sm text-content transition-colors hover:bg-surface-3"
            >
              Choose folder…
            </button>
            <span className="mono min-w-0 flex-1 truncate text-xs text-content-faint">
              {dest ?? "No folder chosen"}
            </span>
          </div>
        </div>
        <div className="flex justify-end pt-1">
          <button
            className={primaryBtn}
            disabled={busy || !dest}
            onClick={() => dest && void clone(url.trim(), dest)}
          >
            {busy ? "Cloning…" : "Clone"}
          </button>
        </div>
      </div>
    </Shell>
  );
}

/**
 * Link the current local repo to an EXISTING GitHub repo (sets origin). Distinct
 * from Publish, which creates a brand-new repo. Used when the repo already lives
 * on GitHub but this local folder isn't wired to it yet.
 */
function ConnectDialog() {
  const connectOrigin = useGithubStore((s) => s.connectOrigin);
  const busy = useGithubStore((s) => s.busy);
  const [url, setUrl] = useState("");

  const valid = !!deriveName(url);

  return (
    <Shell title="Connect to a GitHub repository">
      <div className="space-y-3">
        <p className="text-xs text-content-muted">
          Links this folder to a repository that already exists on GitHub. Nothing
          new is created. After connecting, Pull and Push work as usual.
        </p>
        <div>
          <label className={label}>Repository URL</label>
          <input
            className={input}
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://github.com/owner/repo"
          />
        </div>
        <div className="flex justify-end pt-1">
          <button
            className={primaryBtn}
            disabled={busy || !valid}
            onClick={() => void connectOrigin(url.trim())}
          >
            {busy ? "Connecting…" : "Connect"}
          </button>
        </div>
      </div>
    </Shell>
  );
}

function OpenPrDialog() {
  const repo = useAppStore((s) => s.listing?.repo);
  const openPr = useGithubStore((s) => s.openPr);
  const busy = useGithubStore((s) => s.busy);
  const [title, setTitle] = useState(repo?.branch ? prettifyBranch(repo.branch) : "");
  const [body, setBody] = useState("");

  return (
    <Shell title="Open a pull request">
      <div className="space-y-3">
        <p className="text-xs text-content-muted">
          From <span className="mono text-content">{repo?.branch}</span> into the default branch.
        </p>
        <div>
          <label className={label}>Title</label>
          <input className={input} value={title} onChange={(e) => setTitle(e.target.value)} />
        </div>
        <div>
          <label className={label}>Description (optional)</label>
          <textarea
            className={clsx(input, "resize-none")}
            rows={3}
            value={body}
            onChange={(e) => setBody(e.target.value)}
          />
        </div>
        <div className="flex justify-end pt-1">
          <button
            className={primaryBtn}
            disabled={busy || title.trim().length === 0}
            onClick={() => void openPr(title.trim(), body)}
          >
            {busy ? "Opening…" : "Open pull request"}
          </button>
        </div>
      </div>
    </Shell>
  );
}

function deriveName(url: string): string | null {
  const m = url
    .trim()
    .replace(/\.git$/, "")
    .match(/([^/:]+?)\/?$/);
  return m ? m[1] : null;
}

function prettifyBranch(b: string): string {
  const leaf = b.split("/").pop() ?? b;
  return leaf.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function joinPath(root: string, name: string): string {
  if (isWindowsPath(root)) return `${root.replace(/[\\/]+$/, "")}\\${name}`;
  return `${root.replace(/\/+$/, "")}/${name}`;
}
