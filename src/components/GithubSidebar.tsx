import { useAppStore } from "@/store/useAppStore";
import { useGithubStore, isGithub } from "@/store/useGithubStore";
import type { GhItem, WorkflowRun } from "@/lib/types";

/**
 * Sidebar section for the current GitHub repo: automation run status (Actions),
 * open pull requests, and issues. Clicking an item opens it in the browser.
 * Only rendered when signed in and the repo has a github.com origin.
 */
export function GithubSidebar() {
  const repo = useAppStore((s) => s.listing?.repo);
  const connected = useGithubStore((s) => s.status.connected);
  const prs = useGithubStore((s) => s.prs);
  const issues = useGithubStore((s) => s.issues);
  const runs = useGithubStore((s) => s.runs);
  const loading = useGithubStore((s) => s.listsLoading);
  const open = useGithubStore((s) => s.open);

  if (!connected || !repo || !isGithub(repo.remoteUrl)) return null;

  return (
    <div className="mb-4">
      <p className="px-2 py-1 text-[11px] font-semibold uppercase tracking-wide text-content-faint">
        This repo
      </p>

      <RunsGroup runs={runs} loading={loading} onOpen={open} />
      <ListGroup label="Pull requests" items={prs} loading={loading} onOpen={open} />
      <ListGroup label="Issues" items={issues} loading={loading} onOpen={open} />
    </div>
  );
}

function RunStatusDot({ run }: { run: WorkflowRun }) {
  let cls = "bg-git-modified"; // running / queued
  let title = "In progress";
  if (run.status === "completed") {
    if (run.conclusion === "success") {
      cls = "bg-git-staged";
      title = "Passed";
    } else {
      cls = "bg-git-conflict";
      title = run.conclusion ?? "Failed";
    }
  }
  const pulse = run.status !== "completed";
  return (
    <span
      className={`h-2 w-2 shrink-0 rounded-full ${cls} ${pulse ? "animate-pulse" : ""}`}
      title={title}
    />
  );
}

function RunsGroup({
  runs,
  loading,
  onOpen,
}: {
  runs: WorkflowRun[];
  loading: boolean;
  onOpen: (url: string) => void;
}) {
  return (
    <div className="mb-2">
      <p className="px-2 py-0.5 text-[11px] text-content-faint">Automation runs</p>
      {loading && runs.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">Loading…</p>
      ) : runs.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">No runs yet</p>
      ) : (
        <div className="space-y-0.5">
          {runs.slice(0, 8).map((run, i) => (
            <button
              key={i}
              onClick={() => onOpen(run.url)}
              title={run.name}
              className="group flex w-full items-center gap-2 rounded-lg px-2 py-1 text-left transition-colors hover:bg-surface-2/60"
            >
              <RunStatusDot run={run} />
              <span className="min-w-0 flex-1 truncate text-sm text-content-muted group-hover:text-content">
                {run.name}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ListGroup({
  label,
  items,
  loading,
  onOpen,
}: {
  label: string;
  items: GhItem[];
  loading: boolean;
  onOpen: (url: string) => void;
}) {
  return (
    <div className="mb-2">
      <p className="px-2 py-0.5 text-[11px] text-content-faint">{label}</p>
      {loading && items.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">Loading…</p>
      ) : items.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">None open</p>
      ) : (
        <div className="space-y-0.5">
          {items.map((item) => (
            <button
              key={item.number}
              onClick={() => onOpen(item.url)}
              title={item.title}
              className="group flex w-full items-center gap-2 rounded-lg px-2 py-1 text-left transition-colors hover:bg-surface-2/60"
            >
              <span className="mono shrink-0 text-xs text-content-faint">#{item.number}</span>
              <span className="min-w-0 flex-1 truncate text-sm text-content-muted group-hover:text-content">
                {item.title}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
