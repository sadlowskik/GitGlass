import { useState } from "react";
import clsx from "clsx";
import { useGithubStore } from "@/store/useGithubStore";
import { GithubIcon } from "./icons";

/**
 * Title-bar GitHub control. Signed out: a subtle "Sign in" button. Signed in:
 * the login name with the distinct GitHub-connected accent (green) and a small
 * menu for signing out. We deliberately don't load the remote avatar image —
 * the strict CSP blocks external hosts, and this keeps the app self-contained.
 */
export function GithubButton() {
  const status = useGithubStore((s) => s.status);
  const startSignIn = useGithubStore((s) => s.startSignIn);
  const signOut = useGithubStore((s) => s.signOut);
  const [menuOpen, setMenuOpen] = useState(false);

  if (!status.connected) {
    return (
      <button
        onClick={() => void startSignIn()}
        className="flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
      >
        <GithubIcon className="h-3.5 w-3.5" />
        Sign in
      </button>
    );
  }

  return (
    <div className="relative">
      <button
        onClick={() => setMenuOpen((v) => !v)}
        className="flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-github ring-1 ring-github/30 transition-colors hover:bg-github/10"
        title="GitHub — connected"
      >
        <span className="relative flex">
          <GithubIcon className="h-3.5 w-3.5" />
          <span className="absolute -right-1 -top-1 h-1.5 w-1.5 rounded-full bg-github" />
        </span>
        {status.login}
      </button>

      {menuOpen && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setMenuOpen(false)} />
          <div className="glass absolute right-0 top-full z-50 mt-1 w-40 animate-fade-in rounded-xl p-1 shadow-glass">
            <div className="px-2.5 py-1.5 text-xs text-content-faint">
              Signed in as{" "}
              <span className="font-medium text-content">{status.login}</span>
            </div>
            <button
              onClick={() => {
                setMenuOpen(false);
                void signOut();
              }}
              className={clsx(
                "flex w-full items-center rounded-lg px-2.5 py-1.5 text-left text-sm",
                "text-content transition-colors hover:bg-surface-2",
              )}
            >
              Sign out
            </button>
          </div>
        </>
      )}
    </div>
  );
}
