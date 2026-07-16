import { useMemo } from "react";
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

export function FileList() {
  const listing = useAppStore((s) => s.listing);
  const loading = useAppStore((s) => s.loading);
  const error = useAppStore((s) => s.error);

  const sorted = useMemo(() => (listing ? sortEntries(listing.entries) : []), [listing]);

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
    <div className="flex-1 overflow-y-auto px-2 py-2">
      <div className="mx-auto max-w-3xl">
        {sorted.map((entry) => (
          <FileRow key={entry.path} entry={entry} />
        ))}
      </div>
    </div>
  );
}
