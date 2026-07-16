import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import { useGithubStore } from "@/store/useGithubStore";
import type { RepoSummary } from "@/lib/types";
import { isWindowsPath } from "@/lib/paths";
import { DownloadIcon, GithubIcon } from "./icons";
import { CollapsibleSection } from "./CollapsibleSection";

/**
 * Account-wide list of the signed-in user's own GitHub repositories. Click a
 * repo name to open it on github.com, or the clone button to clone it locally
 * in one step (pick a folder → it's cloned and opened).
 */
export function MyGithubRepos() {
  const connected = useGithubStore((s) => s.status.connected);
  const repos = useGithubStore((s) => s.myRepos);
  const open = useGithubStore((s) => s.open);
  const clone = useGithubStore((s) => s.clone);
  const busy = useGithubStore((s) => s.busy);

  if (!connected) return null;

  const cloneRepo = async (r: RepoSummary) => {
    const picked = await openFolderDialog({
      directory: true,
      multiple: false,
      title: `Clone ${r.name} into…`,
    });
    if (typeof picked === "string") {
      await clone(r.cloneUrl, joinPath(picked, r.name));
    }
  };

  return (
    <CollapsibleSection title="Your repositories" storageKey="repos" count={repos.length}>
      {repos.length === 0 ? (
        <p className="px-2 py-1 text-xs text-content-faint">Loading…</p>
      ) : (
        <div className="max-h-56 space-y-0.5 overflow-y-auto">
          {repos.map((r) => (
            <div
              key={r.fullName}
              className="group flex items-center rounded-lg pr-1 transition-colors hover:bg-surface-2/60"
            >
              <button
                onClick={() => open(r.htmlUrl)}
                title={`${r.fullName} — open on GitHub`}
                className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1 text-left"
              >
                <GithubIcon className="h-3.5 w-3.5 shrink-0 text-content-faint" />
                <span className="min-w-0 flex-1 truncate text-sm text-content-muted group-hover:text-content">
                  {r.name}
                </span>
                {r.private && (
                  <span className="shrink-0 rounded bg-surface-2 px-1 py-0.5 text-[9px] uppercase text-content-faint">
                    private
                  </span>
                )}
              </button>
              <button
                onClick={() => void cloneRepo(r)}
                disabled={busy}
                title={`Clone ${r.name} locally`}
                className="rounded p-1 text-content-faint opacity-0 transition-opacity hover:text-accent group-hover:opacity-100 disabled:opacity-30"
              >
                <DownloadIcon className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}
    </CollapsibleSection>
  );
}

function joinPath(root: string, name: string): string {
  if (isWindowsPath(root)) return `${root.replace(/[\\/]+$/, "")}\\${name}`;
  return `${root.replace(/\/+$/, "")}/${name}`;
}
