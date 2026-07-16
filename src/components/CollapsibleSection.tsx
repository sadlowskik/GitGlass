import { useState } from "react";
import clsx from "clsx";

/**
 * A sidebar section with a collapsible header. The open/closed state is
 * remembered across restarts in localStorage, keyed by `storageKey`, so a user
 * who hides a section keeps it hidden.
 */
export function CollapsibleSection({
  title,
  storageKey,
  count,
  children,
}: {
  title: string;
  storageKey: string;
  count?: number;
  children: React.ReactNode;
}) {
  const key = `gitglass.collapse.${storageKey}`;
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    try {
      return localStorage.getItem(key) === "1";
    } catch {
      return false;
    }
  });

  const toggle = () => {
    setCollapsed((c) => {
      const next = !c;
      try {
        localStorage.setItem(key, next ? "1" : "0");
      } catch {
        // ignore storage failures
      }
      return next;
    });
  };

  return (
    <div className="mb-4">
      <button
        onClick={toggle}
        className="flex w-full items-center gap-1 rounded-md px-1.5 py-1 text-[11px] font-semibold uppercase tracking-wide text-content-faint transition-colors hover:text-content-muted"
      >
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.4"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={clsx("shrink-0 transition-transform", collapsed ? "-rotate-90" : "rotate-0")}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
        <span className="flex-1 text-left">{title}</span>
        {count !== undefined && count > 0 && (
          <span className="rounded-full bg-surface-2 px-1.5 text-[10px] tabular-nums text-content-faint">
            {count}
          </span>
        )}
      </button>
      {!collapsed && <div className="mt-0.5">{children}</div>}
    </div>
  );
}
