import { useEffect, useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import type { AppError, Finding } from "@/lib/types";
import { baseName } from "@/lib/paths";
import { CheckIcon, XIcon } from "./icons";

const label = "mb-1 block text-xs font-medium text-content-muted";
const input =
  "w-full select-text rounded-lg border border-white/10 bg-surface-0 px-2.5 py-1.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-accent";

/**
 * "Make it a repo" — turns a plain folder into a local Git repository with a
 * first commit. Entirely offline: no GitHub account, nothing uploaded. Doubles
 * as first-run identity setup, since a user with no GitHub also has no fallback
 * name/email for commits.
 */
export function InitRepoDialog() {
  const open = useAppStore((s) => s.initRepoOpen);
  const setOpen = useAppStore((s) => s.setInitRepoOpen);
  const folder = useAppStore((s) => s.listing?.path);
  const refresh = useAppStore((s) => s.refresh);
  const notify = useAppStore((s) => s.notify);

  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [needsIdentity, setNeedsIdentity] = useState(false);
  const [busy, setBusy] = useState(false);

  const [scan, setScan] = useState<Finding[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [overridePhrase, setOverridePhrase] = useState("");

  useEffect(() => {
    if (!open) return;
    setScan(null);
    setOverridePhrase("");
    api
      .gitIdentity()
      .then((id) => {
        setName(id.name ?? "");
        setEmail(id.email ?? "");
        setNeedsIdentity(!id.name || !id.email);
      })
      .catch(() => setNeedsIdentity(true));
  }, [open]);

  if (!open || !folder) return null;

  const hasSecrets = !!scan && scan.length > 0;
  const identityOk = name.trim().length > 0 && email.trim().includes("@");
  const canOverride = overridePhrase.trim().toLowerCase() === "track anyway";

  const runScan = async () => {
    setScanning(true);
    setScan(null);
    try {
      setScan(await api.scanFolder(folder));
    } catch (e) {
      notify({ kind: "error", title: (e as AppError)?.message ?? "Scan failed" });
    } finally {
      setScanning(false);
    }
  };

  const doInit = async (allowSecrets: boolean) => {
    setBusy(true);
    try {
      await api.initRepo(folder, name.trim() || null, email.trim() || null, allowSecrets);
      setOpen(false);
      await refresh();
      notify({ kind: "success", title: `“${baseName(folder)}” is now tracked by Git` });
    } catch (e) {
      notify({
        kind: "error",
        title: (e as AppError)?.message ?? "Couldn’t set it up",
        detail: (e as AppError)?.detail,
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
      <div className="glass w-full max-w-md animate-slide-up rounded-2xl p-5">
        <div className="mb-1 flex items-center gap-2">
          <h2 className="flex-1 text-lg font-semibold text-content-strong">Track this folder</h2>
          <button
            onClick={() => setOpen(false)}
            className="rounded-lg p-1 text-content-faint transition-colors hover:bg-surface-2 hover:text-content"
            aria-label="Close"
          >
            <XIcon className="h-4 w-4" />
          </button>
        </div>
        <p className="mb-4 text-xs text-content-muted">
          GitGlass will start keeping a history of{" "}
          <span className="mono text-content">{baseName(folder)}</span> on this computer, so you can
          save versions and undo mistakes.{" "}
          <span className="font-medium text-content">Nothing is uploaded</span> — no GitHub account
          needed. You can publish it later if you want.
        </p>

        <div className="space-y-3">
          {needsIdentity && (
            <p className="rounded-lg bg-surface-0/60 p-2.5 text-xs text-content-muted">
              Git labels each saved version with who made it. This is stored on your computer only.
            </p>
          )}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className={label}>Your name</label>
              <input
                className={input}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Ada Lovelace"
              />
            </div>
            <div>
              <label className={label}>Your email</label>
              <input
                className={input}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="ada@example.com"
              />
            </div>
          </div>

          {/* Optional secret check — nothing leaves the machine, but a heads-up
              here beats discovering it at publish time. */}
          <div className="rounded-lg border border-white/10 p-2.5">
            <div className="flex items-center gap-2">
              <button
                onClick={() => void runScan()}
                disabled={scanning}
                className="rounded-lg bg-surface-2 px-2.5 py-1.5 text-xs font-medium text-content transition-colors hover:bg-surface-3 disabled:opacity-50"
              >
                {scanning ? "Scanning…" : "Check for secrets"}
              </button>
              <span className="text-xs text-content-faint">Optional — nothing is uploaded.</span>
            </div>

            {scan && !hasSecrets && (
              <p className="mt-2 flex items-center gap-1.5 text-xs font-medium text-git-staged">
                <CheckIcon className="h-3.5 w-3.5" /> No secrets found.
              </p>
            )}

            {hasSecrets && (
              <div className="mt-2">
                <p className="mb-1.5 text-xs font-medium text-git-conflict">
                  {scan!.length} possible secret{scan!.length === 1 ? "" : "s"} found.
                </p>
                <div className="max-h-32 space-y-1 overflow-y-auto rounded-lg bg-surface-0/60 p-1.5">
                  {scan!.map((f, i) => (
                    <div key={i} className="flex items-center gap-2 px-1 py-0.5">
                      <span className="shrink-0 rounded-full bg-git-conflict/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-git-conflict">
                        {f.rule}
                      </span>
                      <span className="mono min-w-0 flex-1 truncate text-xs text-content-muted">
                        {f.path}
                        <span className="text-content-faint">:{f.line}</span>
                      </span>
                    </div>
                  ))}
                </div>
                <div className="mt-2">
                  <p className="mb-1.5 text-[11px] text-content-muted">
                    Reviewed and safe? Type{" "}
                    <span className="mono font-semibold text-content-strong">track anyway</span>.
                  </p>
                  <div className="flex items-center gap-2">
                    <input
                      value={overridePhrase}
                      onChange={(e) => setOverridePhrase(e.target.value)}
                      placeholder="track anyway"
                      className={clsx(input, "mono flex-1")}
                    />
                    <button
                      onClick={() => void doInit(true)}
                      disabled={!canOverride || !identityOk || busy}
                      className={clsx(
                        "shrink-0 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors",
                        canOverride && identityOk && !busy
                          ? "bg-git-conflict text-white hover:opacity-90"
                          : "cursor-not-allowed bg-surface-2 text-content-faint",
                      )}
                    >
                      Track anyway
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>

          <div className="flex justify-end pt-1">
            <button
              onClick={() => void doInit(false)}
              disabled={busy || !identityOk || hasSecrets}
              className="rounded-lg bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
            >
              {busy ? "Setting up…" : "Start tracking"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
