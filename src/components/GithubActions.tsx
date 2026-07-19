import { useAppStore } from "@/store/useAppStore";
import { useGithubStore, isGithub } from "@/store/useGithubStore";
import { GithubIcon } from "./icons";

/**
 * Context-aware GitHub action in the toolbar:
 *   - repo not on GitHub yet  → "Publish"
 *   - repo already on GitHub   → "Open PR"
 * Only shown when signed in; otherwise the title-bar sign-in is the entry point.
 */
export function GithubActions() {
  const listing = useAppStore((s) => s.listing);
  const connected = useGithubStore((s) => s.status.connected);
  const openDialog = useGithubStore((s) => s.openDialog);

  // Publish works on ANY folder (it initializes git if needed); Open PR only
  // once the folder is a GitHub repo.
  if (!listing || !connected) return null;

  const onGithub = !!listing.repo && isGithub(listing.repo.remoteUrl);

  if (!onGithub) {
    return (
      <div className="flex shrink-0 items-center gap-1">
        {/* Connect only makes sense for an existing repo (it sets origin); a
            plain folder has nothing to connect, so Publish is the only path. */}
        {listing.repo && (
          <button
            onClick={() => openDialog("connect")}
            title="Link this folder to a repo that already exists on GitHub"
            className="rounded-lg px-2.5 py-1 text-xs font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
          >
            Connect
          </button>
        )}
        <button
          onClick={() => openDialog("publish")}
          className="flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-github ring-1 ring-github/30 transition-colors hover:bg-github/10"
        >
          <GithubIcon className="h-3.5 w-3.5" />
          Publish
        </button>
      </div>
    );
  }

  // On GitHub: offer Open PR + Release (cuts a tagged build of Win + Mac).
  return (
    <div className="flex shrink-0 items-center gap-1">
      <button
        onClick={() => openDialog("pr")}
        className="rounded-lg px-2.5 py-1 text-xs font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
      >
        Open PR
      </button>
      <button
        onClick={() => openDialog("release")}
        title="Tag a version — builds installers if your repo has a release workflow"
        className="flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-github ring-1 ring-github/30 transition-colors hover:bg-github/10"
      >
        <RocketIcon />
        Release
      </button>
    </div>
  );
}

function RocketIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4.5 16.5c-1.5 1.3-2 5-2 5s3.7-.5 5-2c.7-.8.7-2 0-2.8a2 2 0 0 0-3 0ZM12 15l-3-3a22 22 0 0 1 8-10c3 0 5 2 5 5a22 22 0 0 1-10 8ZM9 12H4s.5-2.8 2-4 4 0 4 0M12 15v5s2.8-.5 4-2 0-4 0-4" />
    </svg>
  );
}
