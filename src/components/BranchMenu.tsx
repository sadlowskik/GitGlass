import { useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import type { AppError, BranchInfo } from "@/lib/types";
import { CheckIcon } from "./icons";

/**
 * Branch switcher: the current-branch chip opens a menu to switch, create, and
 * delete branches. Switching with uncommitted changes prompts a confirmation
 * (the backend's safe checkout also refuses to overwrite, so nothing is lost).
 */
export function BranchMenu() {
  const repo = useAppStore((s) => s.listing?.repo);
  const refresh = useAppStore((s) => s.refresh);
  const notify = useAppStore((s) => s.notify);

  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<BranchInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState("");
  const [pendingSwitch, setPendingSwitch] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);

  if (!repo?.branch) return null;
  const root = repo.root;
  const dirty = (repo.stagedCount ?? 0) + (repo.unstagedCount ?? 0) > 0;

  const err = (e: unknown) => notify({ kind: "error", title: (e as AppError)?.message ?? "Failed" });

  const load = async () => {
    setLoading(true);
    try {
      setBranches(await api.listBranches(root));
    } catch (e) {
      err(e);
    } finally {
      setLoading(false);
    }
  };

  const toggle = () => {
    const next = !open;
    setOpen(next);
    setPendingSwitch(null);
    if (next) void load();
  };

  const doSwitch = async (name: string) => {
    setBusy(true);
    try {
      await api.switchBranch(root, name);
      await refresh();
      notify({ kind: "success", title: `Switched to ${name}` });
      setOpen(false);
    } catch (e) {
      err(e);
    } finally {
      setBusy(false);
      setPendingSwitch(null);
    }
  };

  const onPick = (name: string) => {
    if (name === repo.branch) return;
    if (dirty) setPendingSwitch(name);
    else void doSwitch(name);
  };

  const onCreate = async () => {
    const name = newName.trim();
    if (!name) return;
    setBusy(true);
    try {
      await api.createBranch(root, name, true);
      await refresh();
      notify({ kind: "success", title: `Created and switched to ${name}` });
      setNewName("");
      setOpen(false);
    } catch (e) {
      err(e);
    } finally {
      setBusy(false);
    }
  };

  const onDelete = async (name: string, force = false) => {
    setBusy(true);
    try {
      await api.deleteBranch(root, name, force);
      setPendingDelete(null);
      await load();
      notify({ kind: "info", title: `Deleted ${name}` });
    } catch (e) {
      // Unmerged branches are refused once; escalate to an explicit confirm
      // instead of silently force-deleting (which would orphan commits).
      if ((e as AppError)?.kind === "unmerged_branch") {
        setPendingDelete(name);
      } else {
        err(e);
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="relative">
      <button
        onClick={toggle}
        className="mono flex items-center gap-1.5 rounded-md bg-surface-2 px-2 py-1 text-xs text-content-muted transition-colors hover:bg-surface-3 hover:text-content"
        title="Switch branch"
      >
        <BranchGlyph />
        {repo.branch}
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="glass absolute right-0 top-full z-50 mt-1 w-64 animate-fade-in rounded-xl p-1 shadow-glass">
            {pendingSwitch ? (
              <div className="p-2">
                <p className="mb-2 text-xs text-content-muted">
                  You have unsaved changes. Switch to{" "}
                  <span className="mono text-content-strong">{pendingSwitch}</span> anyway?
                  <br />
                  Your changes come with you if they don’t conflict.
                </p>
                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => setPendingSwitch(null)}
                    className="rounded-lg px-2.5 py-1 text-xs text-content-muted hover:bg-surface-2"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => void doSwitch(pendingSwitch)}
                    disabled={busy}
                    className="rounded-lg bg-accent px-2.5 py-1 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
                  >
                    Switch
                  </button>
                </div>
              </div>
            ) : pendingDelete ? (
              <div className="p-2">
                <p className="mb-2 text-xs text-content-muted">
                  <span className="mono text-content-strong">{pendingDelete}</span> has commits
                  that aren’t on your current branch or pushed anywhere. Deleting it{" "}
                  <span className="text-git-conflict">discards those commits for good.</span>
                </p>
                <div className="flex justify-end gap-2">
                  <button
                    onClick={() => setPendingDelete(null)}
                    className="rounded-lg px-2.5 py-1 text-xs text-content-muted hover:bg-surface-2"
                  >
                    Keep it
                  </button>
                  <button
                    onClick={() => void onDelete(pendingDelete, true)}
                    disabled={busy}
                    className="rounded-lg bg-git-conflict px-2.5 py-1 text-xs font-medium text-white hover:brightness-110 disabled:opacity-50"
                  >
                    Delete anyway
                  </button>
                </div>
              </div>
            ) : (
              <>
                <p className="px-2.5 py-1 text-[11px] font-semibold uppercase tracking-wide text-content-faint">
                  Branches
                </p>
                <div className="max-h-56 overflow-y-auto">
                  {loading && <p className="px-2.5 py-1.5 text-xs text-content-faint">Loading…</p>}
                  {branches.map((b) => (
                    <div
                      key={b.name}
                      className={clsx(
                        "group flex items-center rounded-lg pr-1",
                        b.isCurrent ? "bg-surface-2" : "hover:bg-surface-2/60",
                      )}
                    >
                      <button
                        onClick={() => onPick(b.name)}
                        className="flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left"
                      >
                        <span className="flex w-3.5 shrink-0 justify-center">
                          {b.isCurrent && <CheckIcon className="h-3.5 w-3.5 text-git-staged" />}
                        </span>
                        <span
                          className={clsx(
                            "mono min-w-0 flex-1 truncate text-sm",
                            b.isCurrent ? "text-content-strong" : "text-content-muted",
                          )}
                        >
                          {b.name}
                        </span>
                      </button>
                      {!b.isCurrent && (
                        <button
                          onClick={() => void onDelete(b.name)}
                          disabled={busy}
                          title={`Delete ${b.name}`}
                          className="rounded p-1 text-content-faint opacity-0 transition-opacity hover:text-git-conflict group-hover:opacity-100"
                        >
                          <TrashIcon />
                        </button>
                      )}
                    </div>
                  ))}
                </div>

                <div className="mt-1 border-t border-white/10 p-1.5">
                  <input
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && void onCreate()}
                    placeholder="New branch name…"
                    className="mono w-full select-text rounded-lg border border-white/10 bg-surface-0 px-2 py-1.5 text-xs text-content outline-none placeholder:text-content-faint focus:border-accent"
                  />
                </div>
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function BranchGlyph() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <circle cx="6" cy="6" r="2.5" />
      <circle cx="6" cy="18" r="2.5" />
      <circle cx="18" cy="8" r="2.5" />
      <path d="M6 8.5v7M8.5 8H15a3 3 0 0 0 3-3" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2m2 0v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V7" />
    </svg>
  );
}
