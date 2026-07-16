import { useEffect, useRef } from "react";
import clsx from "clsx";

export interface MenuItem {
  label: string;
  onClick: () => void;
  icon?: React.ReactNode;
  danger?: boolean;
  disabled?: boolean;
}

/**
 * A lightweight cursor-anchored context menu. Renders a full-screen catcher so
 * any outside click, right-click, scroll, or Escape closes it. Position is
 * clamped to stay on screen.
 */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Clamp so the menu never overflows the viewport.
  const menuW = 208;
  const menuH = items.length * 34 + 8;
  const left = Math.min(x, window.innerWidth - menuW - 8);
  const top = Math.min(y, window.innerHeight - menuH - 8);

  return (
    <div className="fixed inset-0 z-40" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}>
      <div
        ref={ref}
        style={{ left, top, width: menuW }}
        className="glass absolute animate-fade-in rounded-xl p-1 shadow-glass"
        onClick={(e) => e.stopPropagation()}
      >
        {items.map((item, i) => (
          <button
            key={i}
            disabled={item.disabled}
            onClick={() => {
              item.onClick();
              onClose();
            }}
            className={clsx(
              "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-sm transition-colors",
              item.disabled
                ? "cursor-not-allowed text-content-faint"
                : item.danger
                  ? "text-git-conflict hover:bg-git-conflict/10"
                  : "text-content hover:bg-surface-2",
            )}
          >
            {item.icon && <span className="flex h-4 w-4 items-center justify-center">{item.icon}</span>}
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}
