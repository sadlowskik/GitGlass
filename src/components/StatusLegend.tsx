import { useAppStore } from "@/store/useAppStore";

/**
 * A quiet footer that doubles as the "teach the user" surface: it names the
 * colors so a non-technical user learns the vocabulary without a tutorial.
 * When outside a repo, it explains that too.
 */
export function StatusLegend() {
  const repo = useAppStore((s) => s.listing?.repo);

  const items = [
    { color: "bg-git-untracked", label: "New" },
    { color: "bg-git-modified", label: "Modified" },
    { color: "bg-git-staged", label: "Ready to save" },
    { color: "bg-git-ignored", label: "Ignored" },
  ];

  return (
    <footer className="flex h-8 shrink-0 items-center gap-4 border-t border-white/5 px-4 text-xs text-content-faint">
      {repo ? (
        <>
          <span className="text-content-muted">Git</span>
          {items.map((i) => (
            <span key={i.label} className="flex items-center gap-1.5">
              <span className={`h-1.5 w-1.5 rounded-full ${i.color}`} />
              {i.label}
            </span>
          ))}
        </>
      ) : (
        <span>Not a Git repository — open a folder that contains one to see status.</span>
      )}
    </footer>
  );
}
