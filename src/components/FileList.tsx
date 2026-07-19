import { useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useAppStore } from "@/store/useAppStore";
import type { DirEntry } from "@/lib/types";
import { FileRow } from "./FileRow";
import { ErrorState } from "./ErrorState";

/** Folders first, then files; each group alphabetized, case-insensitive. */
function sortEntries(entries: DirEntry[]): DirEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  });
}

/** Height of one row in px — must match FileRow's padding + content height. */
const ROW_HEIGHT = 34;

export function FileList() {
  const listing = useAppStore((s) => s.listing);
  const loading = useAppStore((s) => s.loading);
  const error = useAppStore((s) => s.error);

  const sorted = useMemo(() => (listing ? sortEntries(listing.entries) : []), [listing]);

  // Only the visible rows are mounted. A dependency folder can hold tens of
  // thousands of entries, and the watcher can re-render this list on every
  // filesystem event — mounting a component per entry made that unaffordable.
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    // Keyed by path so scroll position survives a refresh that reorders rows.
    getItemKey: (i) => sorted[i]?.path ?? i,
  });

  if (error) return <ErrorState error={error} />;

  if (loading && !listing) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-content-faint">
        Loading…
      </div>
    );
  }

  if (listing && sorted.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-1 text-content-faint">
        <p className="text-sm">This folder is empty</p>
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto px-2 py-2">
      <div
        className="relative mx-auto max-w-3xl"
        style={{ height: virtualizer.getTotalSize() }}
      >
        {virtualizer.getVirtualItems().map((item) => (
          <div
            key={item.key}
            ref={virtualizer.measureElement}
            data-index={item.index}
            className="absolute left-0 top-0 w-full"
            style={{ transform: `translateY(${item.start}px)` }}
          >
            <FileRow entry={sorted[item.index]} />
          </div>
        ))}
      </div>
    </div>
  );
}
