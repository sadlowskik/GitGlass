import clsx from "clsx";
import type { GitStatus } from "@/lib/types";

const META: Record<
  Exclude<GitStatus, "clean">,
  { label: string; dot: string; text: string; ring: string }
> = {
  staged: { label: "Staged", dot: "bg-git-staged", text: "text-git-staged", ring: "ring-git-staged/30" },
  modified: { label: "Modified", dot: "bg-git-modified", text: "text-git-modified", ring: "ring-git-modified/30" },
  untracked: { label: "New", dot: "bg-git-untracked", text: "text-git-untracked", ring: "ring-git-untracked/30" },
  ignored: { label: "Ignored", dot: "bg-git-ignored", text: "text-git-ignored", ring: "ring-git-ignored/30" },
  conflict: { label: "Conflict", dot: "bg-git-conflict", text: "text-git-conflict", ring: "ring-git-conflict/30" },
};

/**
 * Inline git status indicator. Two variants:
 *  - "dot": a single colored dot (used in dense file rows)
 *  - "pill": dot + label (used on hover / detail)
 * The color transition is deliberate — staging animates green in M2.
 */
export function StatusBadge({
  status,
  variant = "dot",
  className,
}: {
  status: GitStatus | null;
  variant?: "dot" | "pill";
  className?: string;
}) {
  if (!status || status === "clean") {
    // Reserve layout space so rows don't shift when status appears.
    return <span className={clsx("inline-block h-2 w-2", className)} aria-hidden />;
  }
  const m = META[status];
  if (variant === "dot") {
    return (
      <span
        className={clsx("inline-block h-2 w-2 rounded-full transition-colors duration-300", m.dot, className)}
        title={m.label}
        aria-label={m.label}
      />
    );
  }
  return (
    <span
      className={clsx(
        "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium ring-1 transition-colors duration-300",
        m.text,
        m.ring,
        className,
      )}
    >
      <span className={clsx("h-1.5 w-1.5 rounded-full", m.dot)} />
      {m.label}
    </span>
  );
}
