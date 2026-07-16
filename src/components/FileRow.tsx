import { useState } from "react";
import clsx from "clsx";
import type { DirEntry } from "@/lib/types";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import { CheckIcon, FileIcon, FolderIcon } from "./icons";
import { StatusBadge } from "./StatusBadge";
import { ContextMenu, type MenuItem } from "./ContextMenu";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let n = bytes / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n < 10 ? 1 : 0)} ${units[i]}`;
}

/** A path is "checkable" (stageable) when it's a tracked change or new file. */
function isStageable(entry: DirEntry): boolean {
  const s = entry.gitStatus;
  if (entry.isDir) return entry.hasChanges;
  return s === "modified" || s === "untracked" || s === "conflict" || s === "staged";
}

export function FileRow({ entry }: { entry: DirEntry }) {
  const navigate = useAppStore((s) => s.navigate);
  const stage = useAppStore((s) => s.stage);
  const unstage = useAppStore((s) => s.unstage);
  const notify = useAppStore((s) => s.notify);
  const openDiff = useAppStore((s) => s.openDiff);
  const inRepo = useAppStore((s) => !!s.listing?.repo);

  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  const hasDiff = !entry.isDir && ["modified", "staged", "untracked", "conflict"].includes(
    entry.gitStatus ?? "",
  );

  const onOpen = () => {
    if (entry.isDir) void navigate(entry.path);
    else if (hasDiff) openDiff(entry.path);
  };

  const staged = entry.gitStatus === "staged";
  const dimmed = entry.gitStatus === "ignored";
  const checkable = inRepo && isStageable(entry);

  const toggleStage = () => {
    if (staged) void unstage([entry.path]);
    else void stage([entry.path]);
  };

  const menuItems = (): MenuItem[] => {
    const items: MenuItem[] = [];
    if (hasDiff) {
      items.push({ label: "View changes", onClick: () => openDiff(entry.path) });
    }
    if (checkable) {
      items.push(
        staged
          ? { label: "Unstage", onClick: () => void unstage([entry.path]) }
          : { label: "Stage", onClick: () => void stage([entry.path]) },
      );
    }
    items.push({
      label: "Reveal in File Explorer",
      onClick: () =>
        void api.revealInOs(entry.path).catch((e) => notify({ kind: "error", title: (e as { message?: string })?.message ?? "Couldn’t reveal item" })),
    });
    return items;
  };

  return (
    <>
      <div
        onDoubleClick={onOpen}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        className={clsx(
          "group flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left transition-colors",
          "hover:bg-surface-2",
          dimmed && "opacity-45",
        )}
      >
        {/* Stage checkbox — the one-click "put this on GitHub" affordance. */}
        <span className="flex w-5 shrink-0 justify-center">
          {checkable ? (
            <button
              onClick={toggleStage}
              aria-label={staged ? "Unstage" : "Stage"}
              aria-pressed={staged}
              className={clsx(
                "flex h-4 w-4 items-center justify-center rounded-[5px] border transition-colors duration-200",
                staged
                  ? "border-git-staged bg-git-staged text-white"
                  : "border-content-faint/60 text-transparent hover:border-git-staged",
              )}
            >
              <CheckIcon className="h-3 w-3" strokeWidth={2.6} />
            </button>
          ) : null}
        </span>

        <button onDoubleClick={onOpen} className="flex min-w-0 flex-1 items-center gap-3 text-left">
          <span className="shrink-0 text-content-muted group-hover:text-content">
            {entry.isDir ? (
              <FolderIcon className="h-[18px] w-[18px]" />
            ) : (
              <FileIcon className="h-[18px] w-[18px]" />
            )}
          </span>

          <span className="min-w-0 flex-1 truncate text-sm text-content group-hover:text-content-strong">
            {entry.name}
            {entry.isSymlink && <span className="ml-1 text-xs text-content-faint">↳ link</span>}
          </span>
        </button>

        {entry.isDir && entry.hasChanges && entry.gitStatus === "clean" && (
          <span className="h-1.5 w-1.5 rounded-full bg-git-modified/70" title="Contains changes" />
        )}

        <span className="w-16 shrink-0 text-right text-xs tabular-nums text-content-faint">
          {entry.isDir ? "" : formatSize(entry.sizeBytes)}
        </span>

        <span className="flex w-4 shrink-0 justify-center">
          <StatusBadge status={entry.gitStatus} />
        </span>
      </div>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menuItems()} onClose={() => setMenu(null)} />
      )}
    </>
  );
}
