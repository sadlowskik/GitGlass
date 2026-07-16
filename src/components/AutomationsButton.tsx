import { useAppStore } from "@/store/useAppStore";

/** Toolbar entry point for the Automations dialog. Shown when inside a repo. */
export function AutomationsButton() {
  const inRepo = useAppStore((s) => !!s.listing?.repo);
  const setOpen = useAppStore((s) => s.setAutomationsOpen);

  if (!inRepo) return null;

  return (
    <button
      onClick={() => setOpen(true)}
      title="Schedule a Python task on GitHub Actions"
      className="flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-medium text-content-muted transition-colors hover:bg-surface-2 hover:text-content"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <circle cx="12" cy="12" r="9" />
        <path d="M12 7v5l3 2" strokeLinecap="round" />
      </svg>
      Automations
    </button>
  );
}
