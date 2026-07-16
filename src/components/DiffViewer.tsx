import { useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import type { AppError, DiffHunk, DiffLine, FileDiff } from "@/lib/types";
import { baseName } from "@/lib/paths";
import { highlightLine, langForPath } from "@/lib/highlight";
import { XIcon } from "./icons";

type Mode = "unified" | "split";

export function DiffViewer() {
  const diffPath = useAppStore((s) => s.diffPath);
  const closeDiff = useAppStore((s) => s.closeDiff);
  const repoRoot = useAppStore((s) => s.listing?.repo?.root);

  const [diff, setDiff] = useState<FileDiff | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [mode, setMode] = useState<Mode>("unified");

  useEffect(() => {
    if (!diffPath || !repoRoot) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    setDiff(null);
    api
      .fileDiff(repoRoot, diffPath)
      .then((d) => !cancelled && setDiff(d))
      .catch((e) => !cancelled && setError((e as AppError)?.message ?? "Couldn’t load the diff."))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [diffPath, repoRoot]);

  // Close on Escape.
  useEffect(() => {
    if (!diffPath) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeDiff();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [diffPath, closeDiff]);

  const lang = useMemo(() => (diffPath ? langForPath(diffPath) : undefined), [diffPath]);

  if (!diffPath) return null;

  const empty = diff && !diff.isBinary && diff.hunks.length === 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <div className="glass flex h-[85vh] w-full max-w-5xl flex-col overflow-hidden rounded-2xl">
        <div className="flex shrink-0 items-center gap-3 border-b border-white/10 px-4 py-2.5">
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold text-content-strong">
              {baseName(diffPath)}
            </div>
            <div className="mono truncate text-xs text-content-faint">{diffPath}</div>
          </div>

          <div className="flex shrink-0 items-center overflow-hidden rounded-lg border border-white/10 text-xs">
            <ModeBtn active={mode === "unified"} onClick={() => setMode("unified")}>
              Unified
            </ModeBtn>
            <div className="h-5 w-px bg-white/10" />
            <ModeBtn active={mode === "split"} onClick={() => setMode("split")}>
              Side by side
            </ModeBtn>
          </div>

          <button
            onClick={closeDiff}
            className="rounded-lg p-1.5 text-content-faint transition-colors hover:bg-surface-2 hover:text-content"
            aria-label="Close"
          >
            <XIcon className="h-4 w-4" />
          </button>
        </div>

        <div className="mono min-h-0 flex-1 select-text overflow-auto text-[12.5px] leading-[1.6]">
          {loading && <Centered>Loading changes…</Centered>}
          {error && <Centered className="text-git-conflict">{error}</Centered>}
          {diff?.isBinary && <Centered>Binary file — no preview available.</Centered>}
          {empty && <Centered>No changes to show.</Centered>}
          {diff && !diff.isBinary && diff.hunks.length > 0 && (
            <>
              {diff.hunks.map((hunk, i) =>
                mode === "unified" ? (
                  <UnifiedHunk key={i} hunk={hunk} lang={lang} />
                ) : (
                  <SplitHunk key={i} hunk={hunk} lang={lang} />
                ),
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function ModeBtn({
  children,
  active,
  onClick,
}: {
  children: React.ReactNode;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        "px-2.5 py-1 font-medium transition-colors",
        active ? "bg-surface-2 text-content-strong" : "text-content-muted hover:text-content",
      )}
    >
      {children}
    </button>
  );
}

function Centered({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div className={clsx("flex h-full items-center justify-center text-sm text-content-faint", className)}>
      {children}
    </div>
  );
}

function Code({ content, lang }: { content: string; lang: string | undefined }) {
  return <span dangerouslySetInnerHTML={{ __html: highlightLine(content, lang) || "​" }} />;
}

function bg(origin: DiffLine["origin"]): string {
  if (origin === "add") return "bg-git-staged/10";
  if (origin === "delete") return "bg-git-conflict/10";
  return "";
}
function sign(origin: DiffLine["origin"]): string {
  return origin === "add" ? "+" : origin === "delete" ? "-" : " ";
}

function UnifiedHunk({ hunk, lang }: { hunk: DiffHunk; lang: string | undefined }) {
  return (
    <div>
      <div className="bg-git-untracked/10 px-3 py-0.5 text-git-untracked">{hunk.header}</div>
      {hunk.lines.map((line, i) => (
        <div key={i} className={clsx("flex", bg(line.origin))}>
          <span className="w-10 shrink-0 select-none px-1 text-right text-content-faint">
            {line.oldLineno ?? ""}
          </span>
          <span className="w-10 shrink-0 select-none px-1 text-right text-content-faint">
            {line.newLineno ?? ""}
          </span>
          <span className="w-4 shrink-0 select-none text-center text-content-faint">
            {sign(line.origin)}
          </span>
          <span className="whitespace-pre-wrap break-all pr-3">
            <Code content={line.content} lang={lang} />
          </span>
        </div>
      ))}
    </div>
  );
}

interface Row {
  left?: DiffLine;
  right?: DiffLine;
}

/** Pair deletions with additions so they sit side by side; context spans both. */
function pairRows(lines: DiffLine[]): Row[] {
  const rows: Row[] = [];
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];
  const flush = () => {
    const n = Math.max(dels.length, adds.length);
    for (let i = 0; i < n; i++) rows.push({ left: dels[i], right: adds[i] });
    dels = [];
    adds = [];
  };
  for (const line of lines) {
    if (line.origin === "delete") dels.push(line);
    else if (line.origin === "add") adds.push(line);
    else {
      flush();
      rows.push({ left: line, right: line });
    }
  }
  flush();
  return rows;
}

function SplitHunk({ hunk, lang }: { hunk: DiffHunk; lang: string | undefined }) {
  const rows = useMemo(() => pairRows(hunk.lines), [hunk.lines]);
  return (
    <div>
      <div className="bg-git-untracked/10 px-3 py-0.5 text-git-untracked">{hunk.header}</div>
      {rows.map((row, i) => (
        <div key={i} className="flex">
          <SplitCell line={row.left} side="left" lang={lang} />
          <div className="w-px shrink-0 bg-white/10" />
          <SplitCell line={row.right} side="right" lang={lang} />
        </div>
      ))}
    </div>
  );
}

function SplitCell({
  line,
  side,
  lang,
}: {
  line: DiffLine | undefined;
  side: "left" | "right";
  lang: string | undefined;
}) {
  const origin = line?.origin;
  const shade = side === "left" ? (origin === "delete" ? "bg-git-conflict/10" : "") : origin === "add" ? "bg-git-staged/10" : "";
  const lineno = side === "left" ? line?.oldLineno : line?.newLineno;
  return (
    <div className={clsx("flex w-1/2 min-w-0", shade)}>
      <span className="w-10 shrink-0 select-none px-1 text-right text-content-faint">
        {lineno ?? ""}
      </span>
      <span className="min-w-0 flex-1 whitespace-pre-wrap break-all pr-3">
        {line ? <Code content={line.content} lang={lang} /> : null}
      </span>
    </div>
  );
}
