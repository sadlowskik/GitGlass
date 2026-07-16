import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppStore } from "@/store/useAppStore";
import { MoonIcon, SunIcon } from "./icons";
import { GithubButton } from "./GithubButton";

/**
 * Custom draggable title bar (Tauri window uses decorations: false). The
 * data-tauri-drag-region attribute makes the empty area draggable. The GitHub
 * chip is the "connected state" affordance — greyed until M4 wires OAuth.
 */
export function TitleBar() {
  const theme = useAppStore((s) => s.theme);
  const toggleTheme = useAppStore((s) => s.toggleTheme);
  const win = getCurrentWindow();

  return (
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 select-none items-center justify-between border-b border-white/5 bg-surface-0/80 px-3 backdrop-blur"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 pl-1">
        <div className="h-4 w-4 rounded-[5px] bg-gradient-to-br from-accent to-git-untracked shadow-[0_0_12px_rgb(var(--accent)/0.6)]" />
        <span data-tauri-drag-region className="text-sm font-semibold tracking-tight text-content-strong">
          GitGlass
        </span>
      </div>

      <div className="flex items-center gap-1">
        <GithubButton />

        <button
          onClick={toggleTheme}
          className="rounded-lg p-1.5 text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
          title={theme === "dark" ? "Switch to light" : "Switch to dark"}
        >
          {theme === "dark" ? <SunIcon /> : <MoonIcon />}
        </button>

        <div className="ml-1 flex items-center">
          <WinButton label="Minimize" onClick={() => win.minimize()}>
            <span className="block h-px w-2.5 bg-current" />
          </WinButton>
          <WinButton label="Maximize" onClick={() => win.toggleMaximize()}>
            <span className="block h-2.5 w-2.5 border border-current" />
          </WinButton>
          <WinButton label="Close" onClick={() => win.close()} danger>
            <span className="mono text-sm leading-none">✕</span>
          </WinButton>
        </div>
      </div>
    </header>
  );
}

function WinButton({
  children,
  label,
  onClick,
  danger,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      aria-label={label}
      onClick={onClick}
      className={`flex h-8 w-10 items-center justify-center text-content-muted transition-colors hover:text-content ${
        danger ? "hover:bg-git-conflict hover:text-white" : "hover:bg-surface-2"
      }`}
    >
      {children}
    </button>
  );
}
