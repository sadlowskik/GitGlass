import { useEffect, useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { useGithubStore } from "@/store/useGithubStore";
import { api } from "@/lib/tauri";
import { XIcon } from "./icons";

const label = "mb-1 block text-xs font-medium text-content-muted";
const input =
  "w-full select-text rounded-lg border border-white/10 bg-surface-0 px-2.5 py-1.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-accent";

/**
 * Asks who to sign commits as, when a save failed because Git has no identity.
 *
 * Without this the app is a dead end: the commit fails, and the only way out is
 * `git config` in a terminal — which is exactly what GitGlass exists to avoid.
 * Saving resumes the commit that triggered it, so the user presses save once.
 */
export function IdentityDialog() {
  const prompt = useAppStore((s) => s.identityPrompt);
  const cancel = useAppStore((s) => s.cancelIdentityPrompt);
  const saveAndCommit = useAppStore((s) => s.saveIdentityAndCommit);
  const busy = useAppStore((s) => s.busy);
  const ghLogin = useGithubStore((s) => s.status.login);

  const [name, setName] = useState("");
  const [email, setEmail] = useState("");

  useEffect(() => {
    if (!prompt) return;
    // Prefill from whatever we already know: any half-set Git config first,
    // then the signed-in GitHub account. Their noreply address keeps the real
    // one off public commits, which is the safer default to suggest.
    api
      .gitIdentity()
      .then((id) => {
        setName(id.name ?? ghLogin ?? "");
        setEmail(id.email ?? (ghLogin ? `${ghLogin}@users.noreply.github.com` : ""));
      })
      .catch(() => {
        setName(ghLogin ?? "");
        setEmail(ghLogin ? `${ghLogin}@users.noreply.github.com` : "");
      });
  }, [prompt, ghLogin]);

  if (!prompt) return null;

  const ok = name.trim().length > 0 && /.+@.+/.test(email.trim());

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50 p-4">
      <div className="w-full max-w-md rounded-2xl border border-white/10 bg-surface-1 p-4 shadow-2xl">
        <div className="mb-3 flex items-start justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold text-content">Who should sign your work?</h2>
            <p className="mt-1 text-xs text-content-muted">
              Git stamps every save with a name and email. GitGlass will remember this for all your
              repositories — you can change it any time.
            </p>
          </div>
          <button
            onClick={cancel}
            className="rounded-lg p-1 text-content-muted hover:bg-white/5 hover:text-content"
            aria-label="Close"
          >
            <XIcon className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3">
          <div>
            <label className={label} htmlFor="identity-name">
              Name
            </label>
            <input
              id="identity-name"
              className={input}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Ada Lovelace"
              autoFocus
            />
          </div>
          <div>
            <label className={label} htmlFor="identity-email">
              Email
            </label>
            <input
              id="identity-email"
              className={input}
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
            />
            {ghLogin && email.endsWith("@users.noreply.github.com") && (
              <p className="mt-1 text-[11px] text-content-faint">
                GitHub&rsquo;s private address — keeps your real email out of public commits.
              </p>
            )}
          </div>
        </div>

        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={cancel}
            className="rounded-lg px-3 py-1.5 text-sm text-content-muted hover:bg-white/5"
          >
            Cancel
          </button>
          <button
            disabled={!ok || busy}
            onClick={() => saveAndCommit(name, email)}
            className={clsx(
              "rounded-lg px-3 py-1.5 text-sm font-medium",
              ok && !busy
                ? "bg-accent text-white hover:brightness-110"
                : "cursor-not-allowed bg-white/5 text-content-faint",
            )}
          >
            {busy ? "Saving…" : "Save & continue"}
          </button>
        </div>
      </div>
    </div>
  );
}
