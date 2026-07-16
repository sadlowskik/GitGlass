import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { useGithubStore } from "@/store/useGithubStore";
import { baseName } from "@/lib/paths";
import { FolderIcon, GithubIcon, PinIcon } from "./icons";
import { GithubSidebar } from "./GithubSidebar";
import { MyGithubRepos } from "./MyGithubRepos";
import { AccountAutomations } from "./AccountAutomations";

export function Sidebar() {
  const pinned = useAppStore((s) => s.pinned);
  const recent = useAppStore((s) => s.recent);
  const navigate = useAppStore((s) => s.navigate);
  const pin = useAppStore((s) => s.pin);
  const unpin = useAppStore((s) => s.unpin);
  const listing = useAppStore((s) => s.listing);
  const openGithubDialog = useGithubStore((s) => s.openDialog);

  const currentRepo = listing?.repo?.root;
  const currentIsPinned = !!currentRepo && pinned.some((p) => p.path === currentRepo);

  const openFolder = async () => {
    const selected = await open({ directory: true, multiple: false, title: "Open folder" });
    if (typeof selected === "string") void navigate(selected);
  };

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r border-white/5 bg-surface-1/40">
      <div className="space-y-2 p-3">
        <button
          onClick={openFolder}
          className="flex w-full items-center justify-center gap-2 rounded-lg bg-accent/90 px-3 py-2 text-sm font-medium text-white shadow-sm transition-opacity hover:opacity-90"
        >
          <FolderIcon className="h-4 w-4" />
          Open folder
        </button>
        <button
          onClick={() => openGithubDialog("clone")}
          className="flex w-full items-center justify-center gap-2 rounded-lg bg-surface-2 px-3 py-2 text-sm font-medium text-content transition-colors hover:bg-surface-3"
        >
          <GithubIcon className="h-4 w-4" />
          Clone from GitHub
        </button>
      </div>

      <nav className="flex-1 overflow-y-auto px-2 pb-3">
        <MyGithubRepos />
        <AccountAutomations />
        <GithubSidebar />
        <Section title="Pinned">
          {pinned.length === 0 && <Empty>Pin a repo to keep it handy</Empty>}
          {pinned.map((p) => (
            <Item
              key={p.path}
              label={p.name}
              active={listing?.repo?.root === p.path}
              onClick={() => navigate(p.path)}
              onUnpin={() => unpin(p.path)}
            />
          ))}
        </Section>

        {currentRepo && !currentIsPinned && (
          <button
            onClick={() =>
              pin({ path: currentRepo, name: baseName(currentRepo), pinnedAtMs: Date.now() })
            }
            className="mx-1 mb-2 flex items-center gap-1.5 rounded-md px-2 py-1 text-xs text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
          >
            <PinIcon className="h-3.5 w-3.5" />
            Pin “{baseName(currentRepo)}”
          </button>
        )}

        <Section title="Recent">
          {recent.length === 0 && <Empty>Repositories you open appear here</Empty>}
          {recent.map((path) => (
            <Item
              key={path}
              label={baseName(path)}
              active={listing?.repo?.root === path}
              onClick={() => navigate(path)}
            />
          ))}
        </Section>
      </nav>
    </aside>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-4">
      <p className="px-2 py-1 text-[11px] font-semibold uppercase tracking-wide text-content-faint">
        {title}
      </p>
      <div className="space-y-0.5">{children}</div>
    </div>
  );
}

function Item({
  label,
  active,
  onClick,
  onUnpin,
}: {
  label: string;
  active?: boolean;
  onClick: () => void;
  onUnpin?: () => void;
}) {
  return (
    <div
      className={clsx(
        "group flex items-center rounded-lg pr-1 transition-colors",
        active ? "bg-surface-2" : "hover:bg-surface-2/60",
      )}
    >
      <button
        onClick={onClick}
        className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left"
      >
        <FolderIcon className="h-4 w-4 shrink-0 text-content-muted" />
        <span
          className={clsx(
            "truncate text-sm",
            active ? "text-content-strong" : "text-content-muted group-hover:text-content",
          )}
        >
          {label}
        </span>
      </button>
      {onUnpin && (
        <button
          onClick={onUnpin}
          title="Unpin"
          className="rounded p-1 text-content-faint opacity-0 transition-opacity hover:text-git-conflict group-hover:opacity-100"
        >
          <PinIcon className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return <p className="px-2 py-1 text-xs text-content-faint">{children}</p>;
}
