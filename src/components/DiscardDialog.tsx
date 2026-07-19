import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { baseName } from "@/lib/paths";
import { XIcon } from "./icons";

/**
 * Confirms discarding uncommitted changes. Discard is irreversible — the edits
 * aren't committed anywhere, so once gone they're gone — hence an explicit
 * confirm (a click, not a typed phrase: it's one file's worth of work, not the
 * repo-wide stakes of the secret override).
 */
export function DiscardDialog() {
  const target = useAppStore((s) => s.discardTarget);
  const cancel = useAppStore((s) => s.cancelDiscard);
  const confirm = useAppStore((s) => s.confirmDiscard);
  const busy = useAppStore((s) => s.busy);

  if (!target) return null;

  const label =
    target.length === 1 ? baseName(target[0]) : `${target.length} files`;

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50 p-4">
      <div className="w-full max-w-md rounded-2xl border border-white/10 bg-surface-1 p-4 shadow-2xl">
        <div className="mb-3 flex items-start justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold text-content">Discard changes?</h2>
            <p className="mt-1 text-xs text-content-muted">
              Your uncommitted changes to{" "}
              <span className="font-medium text-content">{label}</span> will be
              thrown away and the file restored to its last saved version. This
              can&rsquo;t be undone.
            </p>
          </div>
          <button
            onClick={cancel}
            className="rounded-lg p-1 text-content-muted hover:bg-white/5 hover:text-content"
            aria-label="Close"
          >
            <XIcon className="h-4 w-4" />
          </button>
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={cancel}
            className="rounded-lg px-3 py-1.5 text-sm text-content-muted hover:bg-white/5"
          >
            Keep my changes
          </button>
          <button
            disabled={busy}
            onClick={() => void confirm()}
            className={clsx(
              "rounded-lg px-3 py-1.5 text-sm font-medium text-white",
              busy
                ? "cursor-not-allowed bg-git-conflict/50"
                : "bg-git-conflict hover:brightness-110",
            )}
          >
            {busy ? "Discarding…" : "Discard changes"}
          </button>
        </div>
      </div>
    </div>
  );
}
