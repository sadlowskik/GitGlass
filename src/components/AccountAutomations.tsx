import { useGithubStore } from "@/store/useGithubStore";
import { CollapsibleSection } from "./CollapsibleSection";

/**
 * Account-wide list of the user's GitHub Actions workflows (automations),
 * gathered across their repos so they're visible from anywhere — not only when
 * a GitHub repo folder is open. Each has a one-click "Run now" (workflow
 * dispatch) and opens on GitHub when clicked.
 */
export function AccountAutomations() {
  const connected = useGithubStore((s) => s.status.connected);
  const automations = useGithubStore((s) => s.automations);
  const loading = useGithubStore((s) => s.automationsLoading);
  const open = useGithubStore((s) => s.open);
  const runNow = useGithubStore((s) => s.runNow);

  if (!connected) return null;

  return (
    <CollapsibleSection title="Automations" storageKey="automations" count={automations.length}>
      {loading && automations.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">Loading…</p>
      ) : automations.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">
          No automations yet — create one with the Automations button.
        </p>
      ) : (
        <div className="space-y-0.5">
          {automations.map((a) => (
            <div
              key={`${a.repoFullName}#${a.id}`}
              className="group flex items-center rounded-lg pr-1 transition-colors hover:bg-surface-2/60"
            >
              <button
                onClick={() => open(a.htmlUrl)}
                title={`${a.name} — ${a.repoFullName}`}
                className="flex min-w-0 flex-1 flex-col px-2 py-1 text-left"
              >
                <span className="truncate text-sm text-content-muted group-hover:text-content">
                  {a.name}
                </span>
                <span className="mono truncate text-[10px] text-content-faint">{a.repoName}</span>
              </button>
              <button
                onClick={() => void runNow(a)}
                title="Run now"
                className="rounded p-1 text-content-faint opacity-0 transition-opacity hover:text-git-staged group-hover:opacity-100"
              >
                <PlayIcon />
              </button>
            </div>
          ))}
        </div>
      )}
    </CollapsibleSection>
  );
}

function PlayIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" stroke="none">
      <path d="M8 5v14l11-7z" />
    </svg>
  );
}
