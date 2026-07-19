import { useMemo, useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import type { Finding } from "@/lib/types";
import { isWindowsPath } from "@/lib/paths";

const OVERRIDE_PHRASE = "commit anyway";

/**
 * The secret protection gate. Shown when a commit's staged diff contains
 * suspected secrets. Three ways out — none of them a one-click bypass:
 *   1. Cancel (default, safest)
 *   2. Add the offending files to .gitignore and unstage them
 *   3. Override — but only after typing the exact confirmation phrase
 * Findings show the exact file + line with a REDACTED preview (never the raw
 * secret), honoring the same never-leak contract as the scanner.
 */
export function SecretGuardDialog() {
  const findings = useAppStore((s) => s.secretFindings);
  const root = useAppStore((s) => s.listing?.repo?.root);
  const cancel = useAppStore((s) => s.cancelSecretGuard);
  const override = useAppStore((s) => s.confirmOverrideCommit);
  const ignoreSecrets = useAppStore((s) => s.ignoreSecrets);
  const busy = useAppStore((s) => s.busy);
  // Amend is gated too; say which write is being blocked rather than always
  // saying "commit", so the dialog matches the button the user pressed.
  const isAmend = useAppStore((s) => s.secretGuardAction === "amend");

  const [phrase, setPhrase] = useState("");

  const files = useMemo(() => uniqueFiles(findings ?? []), [findings]);

  if (!findings || findings.length === 0) return null;

  const canOverride = phrase.trim().toLowerCase() === OVERRIDE_PHRASE;

  const onIgnore = () => {
    if (!root) return;
    const abs = files.map((f) => joinPath(root, f));
    void ignoreSecrets(abs);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
      <div className="glass w-full max-w-lg animate-slide-up rounded-2xl p-5">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-git-conflict/15 text-git-conflict">
            <ShieldIcon />
          </div>
          <div className="min-w-0">
            <h2 className="text-lg font-semibold text-content-strong">
              Hold on — this looks like a secret
            </h2>
            <p className="mt-0.5 text-sm text-content-muted">
              GitGlass found {findings.length} thing{findings.length === 1 ? "" : "s"} that look like
              API keys, tokens, or passwords in what you’re about to save.{" "}
              {isAmend ? "Folding these into your last commit" : "Committing these"} could expose
              them publicly.
            </p>
          </div>
        </div>

        <div className="mt-4 max-h-52 space-y-1.5 overflow-y-auto rounded-xl bg-surface-0/60 p-2">
          {findings.map((f, i) => (
            <FindingRow key={i} finding={f} />
          ))}
        </div>

        <div className="mt-4 space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <button
              onClick={cancel}
              className="rounded-lg bg-accent px-3.5 py-2 text-sm font-medium text-white transition-opacity hover:opacity-90"
            >
              Cancel & review
            </button>
            <button
              onClick={onIgnore}
              disabled={busy}
              className="rounded-lg bg-surface-2 px-3.5 py-2 text-sm font-medium text-content transition-colors hover:bg-surface-3 disabled:opacity-50"
            >
              Add {files.length} file{files.length === 1 ? "" : "s"} to .gitignore
            </button>
          </div>

          <details className="group rounded-lg border border-white/10 p-3">
            <summary className="cursor-pointer select-none text-xs font-medium text-content-muted">
              I understand the risk — let me {isAmend ? "amend" : "commit"} anyway
            </summary>
            <div className="mt-3">
              <p className="mb-2 text-xs text-content-muted">
                This is intentional and safe. To confirm, type{" "}
                <span className="mono font-semibold text-content-strong">{OVERRIDE_PHRASE}</span>{" "}
                below.
              </p>
              <div className="flex items-center gap-2">
                <input
                  value={phrase}
                  onChange={(e) => setPhrase(e.target.value)}
                  placeholder={OVERRIDE_PHRASE}
                  className="mono flex-1 select-text rounded-lg border border-white/10 bg-surface-0 px-2.5 py-1.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-git-conflict"
                />
                <button
                  onClick={() => void override()}
                  disabled={!canOverride || busy}
                  className={clsx(
                    "rounded-lg px-3.5 py-1.5 text-sm font-medium transition-colors",
                    canOverride && !busy
                      ? "bg-git-conflict text-white hover:opacity-90"
                      : "cursor-not-allowed bg-surface-2 text-content-faint",
                  )}
                >
                  {busy ? "Saving…" : isAmend ? "Amend anyway" : "Commit anyway"}
                </button>
              </div>
            </div>
          </details>
        </div>
      </div>
    </div>
  );
}

function FindingRow({ finding }: { finding: Finding }) {
  const high = finding.severity === "high";
  return (
    <div className="flex items-center gap-2.5 rounded-lg px-2 py-1.5">
      <span
        className={clsx(
          "shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase",
          high ? "bg-git-conflict/15 text-git-conflict" : "bg-git-modified/15 text-git-modified",
        )}
      >
        {finding.rule}
      </span>
      <span className="mono min-w-0 flex-1 truncate text-xs text-content-muted">
        {finding.path}
        <span className="text-content-faint">:{finding.line}</span>
      </span>
      <span className="mono shrink-0 text-xs text-content-faint">{finding.preview}</span>
    </div>
  );
}

function uniqueFiles(findings: Finding[]): string[] {
  return [...new Set(findings.map((f) => f.path))];
}

/** Join a repo root with a repo-relative (forward-slash) path, native-aware. */
function joinPath(root: string, rel: string): string {
  if (isWindowsPath(root)) {
    const r = root.replace(/[\\/]+$/, "");
    return `${r}\\${rel.replace(/\//g, "\\")}`;
  }
  const r = root.replace(/\/+$/, "");
  return `${r}/${rel}`;
}

function ShieldIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M12 3l7 3v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6l7-3Z" />
      <path d="M12 9v3m0 3h.01" />
    </svg>
  );
}
