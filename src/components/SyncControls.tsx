import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { DownloadIcon, UploadIcon } from "./icons";
import { BranchMenu } from "./BranchMenu";

/**
 * Branch + sync state in the toolbar. Shows "N ahead / M behind" and the
 * pull/push controls. When there's no upstream yet, ahead/behind are null and
 * the buttons still work — the backend returns a friendly "not connected to a
 * remote yet" message rather than failing silently.
 */
export function SyncControls() {
  const repo = useAppStore((s) => s.listing?.repo);
  const busy = useAppStore((s) => s.busy);
  const pull = useAppStore((s) => s.pull);
  const push = useAppStore((s) => s.push);

  if (!repo) return null;

  const ahead = repo.ahead ?? 0;
  const behind = repo.behind ?? 0;
  const hasUpstream = repo.ahead !== null;

  return (
    <div className="flex shrink-0 items-center gap-1.5">
      <BranchMenu />

      {hasUpstream && (ahead > 0 || behind > 0) && (
        <span className="flex items-center gap-1 text-xs tabular-nums text-content-muted">
          {ahead > 0 && <span title={`${ahead} to push`}>↑{ahead}</span>}
          {behind > 0 && <span title={`${behind} to pull`}>↓{behind}</span>}
        </span>
      )}

      <div className="ml-0.5 flex items-center overflow-hidden rounded-lg border border-white/10">
        <SyncButton
          label="Pull"
          onClick={() => void pull()}
          disabled={busy}
          badge={behind > 0 ? behind : undefined}
        >
          <DownloadIcon className="h-4 w-4" />
        </SyncButton>
        <div className="h-5 w-px bg-white/10" />
        <SyncButton
          label="Push"
          onClick={() => void push()}
          disabled={busy}
          badge={ahead > 0 ? ahead : undefined}
          accent
        >
          <UploadIcon className="h-4 w-4" />
        </SyncButton>
      </div>
    </div>
  );
}

function SyncButton({
  children,
  label,
  onClick,
  disabled,
  badge,
  accent,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  badge?: number;
  accent?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={label}
      className={clsx(
        "flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium transition-colors",
        disabled
          ? "text-content-faint"
          : accent
            ? "text-content hover:bg-accent hover:text-white"
            : "text-content hover:bg-surface-2",
      )}
    >
      {children}
      <span className="hidden sm:inline">{label}</span>
      {badge !== undefined && (
        <span
          className={clsx(
            "ml-0.5 rounded-full px-1.5 text-[10px] tabular-nums",
            accent ? "bg-accent/20 text-accent" : "bg-git-untracked/20 text-git-untracked",
          )}
        >
          {badge}
        </span>
      )}
    </button>
  );
}

