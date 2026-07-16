import { useEffect, useState } from "react";
import clsx from "clsx";
import { useAppStore } from "@/store/useAppStore";
import { api } from "@/lib/tauri";
import type { AppError } from "@/lib/types";
import { buildCron, describe, isValidCron, WEEKDAYS, type Frequency } from "@/lib/cron";
import { XIcon } from "./icons";

const PY_VERSIONS = ["3.13", "3.12", "3.11", "3.10"];

const FREQ_OPTIONS: { value: Frequency; label: string }[] = [
  { value: "every-15m", label: "Every 15 minutes" },
  { value: "every-30m", label: "Every 30 minutes" },
  { value: "hourly", label: "Every hour" },
  { value: "every-6h", label: "Every 6 hours" },
  { value: "daily", label: "Every day" },
  { value: "weekly", label: "Every week" },
  { value: "custom", label: "Custom (advanced)" },
];

const label = "mb-1 block text-xs font-medium text-content-muted";
const input =
  "w-full select-text rounded-lg border border-white/10 bg-surface-0 px-2.5 py-1.5 text-sm text-content outline-none placeholder:text-content-faint focus:border-accent";

function parentDir(absPath: string): string {
  return absPath.replace(/[\\/][^\\/]+$/, "");
}

export function AutomationsDialog() {
  const open = useAppStore((s) => s.automationsOpen);
  const setOpen = useAppStore((s) => s.setAutomationsOpen);
  const repo = useAppStore((s) => s.listing?.repo);
  const navigate = useAppStore((s) => s.navigate);
  const notify = useAppStore((s) => s.notify);

  const [pyFiles, setPyFiles] = useState<string[]>([]);
  const [workflows, setWorkflows] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);

  const [name, setName] = useState("");
  const [pythonPath, setPythonPath] = useState("");
  const [frequency, setFrequency] = useState<Frequency>("hourly");
  const [time, setTime] = useState("09:00");
  const [weekday, setWeekday] = useState(1);
  const [custom, setCustom] = useState("0 9 * * *");
  const [pythonVersion, setPythonVersion] = useState("3.12");
  const [installReqs, setInstallReqs] = useState(false);

  const root = repo?.root;

  useEffect(() => {
    if (!open || !root) return;
    setLoading(true);
    Promise.all([api.listPythonFiles(root), api.listWorkflows(root), api.hasRequirements(root)])
      .then(([files, wfs, hasReq]) => {
        setPyFiles(files);
        setWorkflows(wfs.map((w) => w.file));
        setInstallReqs(hasReq);
        if (files.length > 0 && !pythonPath) setPythonPath(files[0]);
      })
      .catch((e) => notify({ kind: "error", title: (e as AppError)?.message ?? "Failed to load" }))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, root]);

  if (!open) return null;

  const schedule = { frequency, time, weekday, custom };
  const cron = buildCron(schedule);
  const cronOk = frequency !== "custom" || isValidCron(custom);

  const canCreate = !!root && name.trim().length > 0 && pythonPath.length > 0 && cronOk && !busy;

  const onCreate = async () => {
    if (!root) return;
    setBusy(true);
    try {
      const path = await api.createWorkflow({
        repo: root,
        name: name.trim(),
        pythonPath,
        cron,
        pythonVersion,
        installRequirements: installReqs,
      });
      setOpen(false);
      // Take the user to the workflows folder so the new file is right there to
      // stage → commit → push (which is what turns the automation on).
      await navigate(parentDir(path));
      notify({
        kind: "success",
        title: "Automation created — commit & push it to turn it on",
      });
    } catch (e) {
      notify({ kind: "error", title: (e as AppError)?.message ?? "Couldn’t create it", detail: (e as AppError)?.detail });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
      <div className="glass w-full max-w-lg animate-slide-up rounded-2xl p-5">
        <div className="mb-1 flex items-center gap-2">
          <ClockIcon />
          <h2 className="flex-1 text-lg font-semibold text-content-strong">
            Schedule a Python task
          </h2>
          <button
            onClick={() => setOpen(false)}
            className="rounded-lg p-1 text-content-faint transition-colors hover:bg-surface-2 hover:text-content"
            aria-label="Close"
          >
            <XIcon className="h-4 w-4" />
          </button>
        </div>
        <p className="mb-4 text-xs text-content-muted">
          GitGlass writes a GitHub Actions workflow for you. Commit & push it, and GitHub runs your
          script on the schedule — no servers, no YAML.
        </p>

        {workflows.length > 0 && (
          <div className="mb-3 rounded-lg bg-surface-0/60 p-2">
            <p className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-content-faint">
              Existing automations
            </p>
            <div className="flex flex-wrap gap-1.5">
              {workflows.map((w) => (
                <span key={w} className="mono rounded-md bg-surface-2 px-1.5 py-0.5 text-xs text-content-muted">
                  {w}
                </span>
              ))}
            </div>
          </div>
        )}

        {loading ? (
          <p className="py-6 text-center text-sm text-content-faint">Scanning the repository…</p>
        ) : pyFiles.length === 0 ? (
          <p className="rounded-lg bg-surface-0/60 p-4 text-center text-sm text-content-muted">
            No Python (<span className="mono">.py</span>) files found in this repository. Add your
            script here first, then come back.
          </p>
        ) : (
          <div className="space-y-3">
            <div>
              <label className={label}>Name</label>
              <input
                className={input}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Nightly report"
              />
            </div>

            <div>
              <label className={label}>Run this script</label>
              <select className={input} value={pythonPath} onChange={(e) => setPythonPath(e.target.value)}>
                {pyFiles.map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
              </select>
            </div>

            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className={label}>How often</label>
                <select
                  className={input}
                  value={frequency}
                  onChange={(e) => setFrequency(e.target.value as Frequency)}
                >
                  {FREQ_OPTIONS.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className={label}>Python version</label>
                <select
                  className={input}
                  value={pythonVersion}
                  onChange={(e) => setPythonVersion(e.target.value)}
                >
                  {PY_VERSIONS.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {(frequency === "daily" || frequency === "weekly") && (
              <div className="grid grid-cols-2 gap-3">
                {frequency === "weekly" && (
                  <div>
                    <label className={label}>Day</label>
                    <select
                      className={input}
                      value={weekday}
                      onChange={(e) => setWeekday(parseInt(e.target.value, 10))}
                    >
                      {WEEKDAYS.map((d, i) => (
                        <option key={d} value={i}>
                          {d}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
                <div>
                  <label className={label}>Time (your local time)</label>
                  <input type="time" className={input} value={time} onChange={(e) => setTime(e.target.value)} />
                </div>
              </div>
            )}

            {frequency === "custom" && (
              <div>
                <label className={label}>Cron expression (UTC)</label>
                <input
                  className={clsx(input, "mono", !cronOk && "border-git-conflict")}
                  value={custom}
                  onChange={(e) => setCustom(e.target.value)}
                  placeholder="0 9 * * 1"
                />
                {!cronOk && <p className="mt-1 text-xs text-git-conflict">Needs 5 space-separated fields.</p>}
              </div>
            )}

            <label className="flex cursor-pointer items-center gap-2 text-sm text-content">
              <input
                type="checkbox"
                checked={installReqs}
                onChange={(e) => setInstallReqs(e.target.checked)}
                className="h-4 w-4 accent-accent"
              />
              Install <span className="mono">requirements.txt</span> before running
            </label>

            <div className="rounded-lg bg-surface-0/60 p-2.5">
              <p className="text-xs text-content-muted">{describe(schedule)}</p>
              <p className="mono mt-1 text-xs text-content-faint">cron: {cron || "—"}</p>
            </div>

            <div className="flex justify-end pt-1">
              <button
                onClick={() => void onCreate()}
                disabled={!canCreate}
                className="rounded-lg bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
              >
                {busy ? "Creating…" : "Create automation"}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ClockIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" className="text-content-muted">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" strokeLinecap="round" />
    </svg>
  );
}
