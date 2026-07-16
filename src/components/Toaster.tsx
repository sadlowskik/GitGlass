import { useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import type { Toast } from "@/store/useAppStore";
import { CheckIcon, XIcon } from "./icons";

/**
 * Bottom-right toast stack. Success/info self-dismiss; errors persist and show
 * an optional "details" disclosure carrying the raw backend detail — the same
 * never-show-raw-errors contract as ErrorState, in transient form.
 */
export function Toaster() {
  const toasts = useAppStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
      {toasts.map((t) => (
        <ToastCard key={t.id} toast={t} />
      ))}
    </div>
  );
}

function ToastCard({ toast }: { toast: Toast }) {
  const dismiss = useAppStore((s) => s.dismissToast);
  const [showDetail, setShowDetail] = useState(false);
  const isError = toast.kind === "error";

  return (
    <div
      className={clsx(
        "glass pointer-events-auto animate-slide-up rounded-xl p-3 pr-2 shadow-glass",
        isError && "ring-1 ring-git-conflict/40",
      )}
      role={isError ? "alert" : "status"}
    >
      <div className="flex items-start gap-2.5">
        <span
          className={clsx(
            "mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full",
            isError ? "bg-git-conflict/15 text-git-conflict" : "bg-git-staged/15 text-git-staged",
          )}
        >
          {isError ? <XIcon className="h-3.5 w-3.5" /> : <CheckIcon className="h-3.5 w-3.5" />}
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-content-strong">{toast.title}</p>
          {isError && toast.detail && (
            <>
              <button
                onClick={() => setShowDetail((v) => !v)}
                className="mt-1 text-xs text-content-faint underline-offset-2 hover:underline"
              >
                {showDetail ? "Hide" : "Show"} details
              </button>
              {showDetail && (
                <pre className="mono mt-1 max-h-24 select-text overflow-auto rounded-md bg-surface-0 p-1.5 text-[11px] text-content-muted">
                  {toast.detail}
                </pre>
              )}
            </>
          )}
        </div>
        <button
          onClick={() => dismiss(toast.id)}
          className="rounded-md p-1 text-content-faint transition-colors hover:bg-surface-2 hover:text-content"
          aria-label="Dismiss"
        >
          <XIcon className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
