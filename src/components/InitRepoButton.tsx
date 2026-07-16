import { useAppStore } from "@/store/useAppStore";

/**
 * Shown when the current folder isn't tracked by Git yet. Unlike Publish, this
 * needs no GitHub account — it's the local-only "start keeping history" entry
 * point.
 */
export function InitRepoButton() {
  const inFolder = useAppStore((s) => !!s.listing);
  const isRepo = useAppStore((s) => !!s.listing?.repo);
  const setOpen = useAppStore((s) => s.setInitRepoOpen);

  if (!inFolder || isRepo) return null;

  return (
    <button
      onClick={() => setOpen(true)}
      title="Start tracking this folder with Git (stays on your computer)"
      className="flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <circle cx="6" cy="6" r="2.5" />
        <circle cx="6" cy="18" r="2.5" />
        <circle cx="18" cy="8" r="2.5" />
        <path d="M6 8.5v7M8.5 8H15a3 3 0 0 0 3-3" strokeLinecap="round" />
      </svg>
      Make it a repo
    </button>
  );
}
