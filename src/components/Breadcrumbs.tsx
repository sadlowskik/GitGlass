import { useAppStore } from "@/store/useAppStore";
import { segments } from "@/lib/paths";
import { ArrowUp, ChevronRight, RefreshIcon } from "./icons";
import { SyncControls } from "./SyncControls";
import { GithubActions } from "./GithubActions";
import { AutomationsButton } from "./AutomationsButton";
import { InitRepoButton } from "./InitRepoButton";

export function Breadcrumbs() {
  const listing = useAppStore((s) => s.listing);
  const navigate = useAppStore((s) => s.navigate);
  const goUp = useAppStore((s) => s.goUp);
  const refresh = useAppStore((s) => s.refresh);
  const loading = useAppStore((s) => s.loading);

  const crumbs = listing ? segments(listing.path) : [];

  return (
    <div className="flex h-11 shrink-0 items-center gap-1 border-b border-white/5 px-3">
      <button
        onClick={goUp}
        disabled={!listing?.parent}
        className="rounded-lg p-1.5 text-content-muted transition-colors hover:bg-surface-2 hover:text-content disabled:opacity-30"
        title="Up one level"
      >
        <ArrowUp />
      </button>
      <button
        onClick={refresh}
        className="rounded-lg p-1.5 text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
        title="Refresh"
      >
        <RefreshIcon className={loading ? "animate-spin" : ""} />
      </button>

      <div className="mx-1 flex min-w-0 flex-1 items-center overflow-x-auto whitespace-nowrap px-1">
        {crumbs.map((c, i) => (
          <div key={c.path} className="flex items-center">
            {i > 0 && <ChevronRight className="mx-0.5 h-3.5 w-3.5 text-content-faint" />}
            <button
              onClick={() => navigate(c.path)}
              className="rounded-md px-1.5 py-0.5 text-sm text-content-muted transition-colors hover:bg-surface-2 hover:text-content-strong"
            >
              {c.label}
            </button>
          </div>
        ))}
      </div>

      <InitRepoButton />
      <AutomationsButton />
      <GithubActions />
      <SyncControls />
    </div>
  );
}
