import { useState } from "react";
import type { AppError } from "@/lib/types";
import { useAppStore } from "@/store/useAppStore";

/**
 * The single place errors surface in the explorer pane. Messages arrive
 * already-friendly from the backend (never a raw git string). Raw developer
 * detail is tucked behind a disclosure so it never leaks into the main UI.
 * This is the seed of the M6 "friendly error translation" layer.
 */
export function ErrorState({ error }: { error: AppError }) {
  const [showDetail, setShowDetail] = useState(false);
  const refresh = useAppStore((s) => s.refresh);
  const goUp = useAppStore((s) => s.goUp);

  return (
    <div className="flex flex-1 items-center justify-center p-6">
      <div className="glass w-full max-w-md rounded-2xl p-6 text-center">
        <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-git-conflict/15 text-git-conflict">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
            <path d="M12 9v4m0 4h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" />
          </svg>
        </div>
        <h2 className="mb-1 text-base font-semibold text-content-strong">{error.message}</h2>
        <p className="mb-4 text-sm text-content-muted">
          {friendlyHint(error.kind)}
        </p>

        <div className="flex items-center justify-center gap-2">
          <button
            onClick={() => refresh()}
            className="rounded-lg bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90"
          >
            Try again
          </button>
          <button
            onClick={() => goUp()}
            className="rounded-lg bg-surface-2 px-3.5 py-1.5 text-sm font-medium text-content transition-colors hover:bg-surface-3"
          >
            Go back
          </button>
        </div>

        {error.detail && (
          <div className="mt-4 text-left">
            <button
              onClick={() => setShowDetail((v) => !v)}
              className="text-xs text-content-faint underline-offset-2 hover:underline"
            >
              {showDetail ? "Hide" : "Show"} technical details
            </button>
            {showDetail && (
              <pre className="mono mt-2 max-h-32 select-text overflow-auto rounded-lg bg-surface-0 p-2 text-xs text-content-muted">
                {error.detail}
              </pre>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function friendlyHint(kind: string): string {
  switch (kind) {
    case "not_found":
      return "That folder may have been moved or renamed.";
    case "permission":
      return "GitGlass doesn't have permission to open this folder.";
    case "not_a_repo":
      return "This folder isn't a Git repository yet.";
    default:
      return "You can retry, or step back to the previous folder.";
  }
}
