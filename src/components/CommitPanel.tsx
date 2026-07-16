import { useEffect } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { CommitIcon } from "./icons";

/**
 * The commit surface. It lives at the bottom of the explorer and slides up when
 * there's anything to save. Two tiers:
 *   - a compact bar showing staged/unstaged counts + "Stage all"
 *   - an expanded panel (message + Save) once the user commits to committing
 * A non-technical user reads "Save 3 changes", never "commit staged index".
 */
export function CommitPanel() {
  const repo = useAppStore((s) => s.listing?.repo);
  const commitOpen = useAppStore((s) => s.commitOpen);
  const setCommitOpen = useAppStore((s) => s.setCommitOpen);
  const stage = useAppStore((s) => s.stage);
  const unstage = useAppStore((s) => s.unstage);
  const requestCommit = useAppStore((s) => s.requestCommit);
  const message = useAppStore((s) => s.commitMessage);
  const setMessage = useAppStore((s) => s.setCommitMessage);
  const busy = useAppStore((s) => s.busy);

  const staged = repo?.stagedCount ?? 0;
  const unstaged = repo?.unstagedCount ?? 0;
  const root = repo?.root;

  // Collapse the expanded editor whenever there's nothing left staged.
  useEffect(() => {
    if (staged === 0 && commitOpen) setCommitOpen(false);
  }, [staged, commitOpen, setCommitOpen]);

  if (!repo || (staged === 0 && unstaged === 0)) return null;

  const onCommit = () => void requestCommit();

  return (
    <div className="animate-slide-up border-t border-white/5 bg-surface-1/70 backdrop-blur-xl">
      <div className="mx-auto max-w-3xl px-4 py-2.5">
        <div className="flex items-center gap-3">
          <div className="flex items-center gap-2 text-sm">
            <span className="flex items-center gap-1.5 font-medium text-content-strong">
              <span className="h-2 w-2 rounded-full bg-git-staged" />
              {staged} ready
            </span>
            {unstaged > 0 && (
              <span className="flex items-center gap-1.5 text-content-muted">
                <span className="h-2 w-2 rounded-full bg-git-modified" />
                {unstaged} not staged
              </span>
            )}
          </div>

          <div className="ml-auto flex items-center gap-2">
            {unstaged > 0 && root && (
              <button
                onClick={() => void stage([root])}
                className="rounded-lg px-2.5 py-1.5 text-sm font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
              >
                Stage all
              </button>
            )}
            {staged > 0 && root && (
              <button
                onClick={() => void unstage([root])}
                className="rounded-lg px-2.5 py-1.5 text-sm text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
              >
                Unstage all
              </button>
            )}
            {!commitOpen && staged > 0 && (
              <button
                onClick={() => setCommitOpen(true)}
                className="flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90"
              >
                <CommitIcon className="h-4 w-4" />
                Save {staged} {staged === 1 ? "change" : "changes"}
              </button>
            )}
          </div>
        </div>

        {commitOpen && staged > 0 && (
          <div className="mt-2.5 animate-fade-in">
            <textarea
              autoFocus
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void onCommit();
              }}
              placeholder="Describe what you changed (e.g. “Add contact form”)"
              rows={2}
              className="mono w-full resize-none select-text rounded-lg border border-white/10 bg-surface-0 p-2.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-accent"
            />
            <div className="mt-2 flex items-center justify-between">
              <span className="text-xs text-content-faint">Tip: ⌘/Ctrl + Enter to save</span>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setCommitOpen(false)}
                  className="rounded-lg px-3 py-1.5 text-sm text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
                >
                  Cancel
                </button>
                <button
                  onClick={() => void onCommit()}
                  disabled={busy || message.trim().length === 0}
                  className={clsx(
                    "flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity",
                    busy || message.trim().length === 0 ? "opacity-40" : "hover:opacity-90",
                  )}
                >
                  <CommitIcon className="h-4 w-4" />
                  {busy ? "Saving…" : "Save"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
