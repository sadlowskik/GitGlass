// Build GitHub Actions cron expressions from a friendly schedule picker, and
// describe them back in plain English. GitHub runs cron in UTC, so daily/weekly
// times chosen in the user's local zone are converted to UTC here.

export type Frequency =
  | "every-15m"
  | "every-30m"
  | "hourly"
  | "every-6h"
  | "daily"
  | "weekly"
  | "custom";

export interface ScheduleInput {
  frequency: Frequency;
  /** "HH:MM" local time, for daily/weekly. */
  time: string;
  /** 0=Sunday … 6=Saturday, for weekly. */
  weekday: number;
  /** raw cron, for "custom". */
  custom: string;
}

const DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

function parseTime(time: string): { h: number; m: number } {
  const [h, m] = time.split(":").map((n) => parseInt(n, 10));
  return { h: isNaN(h) ? 9 : h, m: isNaN(m) ? 0 : m };
}

/** Convert a local time (and optional weekday) to UTC minute/hour/weekday. */
function toUtc(h: number, m: number, weekday?: number): { m: number; h: number; dow: number } {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  if (weekday !== undefined) {
    const shift = (weekday - d.getDay() + 7) % 7;
    d.setDate(d.getDate() + shift);
  }
  return { m: d.getUTCMinutes(), h: d.getUTCHours(), dow: d.getUTCDay() };
}

export function buildCron(input: ScheduleInput): string {
  const { h, m } = parseTime(input.time);
  switch (input.frequency) {
    case "every-15m":
      return "*/15 * * * *";
    case "every-30m":
      return "*/30 * * * *";
    case "hourly":
      return "0 * * * *";
    case "every-6h":
      return "0 */6 * * *";
    case "daily": {
      const u = toUtc(h, m);
      return `${u.m} ${u.h} * * *`;
    }
    case "weekly": {
      const u = toUtc(h, m, input.weekday);
      return `${u.m} ${u.h} * * ${u.dow}`;
    }
    case "custom":
      return input.custom.trim();
  }
}

/** Human summary shown under the picker, including the UTC note. */
export function describe(input: ScheduleInput): string {
  const { h, m } = parseTime(input.time);
  const t = `${input.time}`;
  switch (input.frequency) {
    case "every-15m":
      return "Runs every 15 minutes.";
    case "every-30m":
      return "Runs every 30 minutes.";
    case "hourly":
      return "Runs at the top of every hour.";
    case "every-6h":
      return "Runs every 6 hours.";
    case "daily": {
      const u = toUtc(h, m);
      return `Runs every day at ${t} your time (${pad(u.h)}:${pad(u.m)} UTC).`;
    }
    case "weekly": {
      const u = toUtc(h, m, input.weekday);
      return `Runs every ${DAYS[input.weekday]} at ${t} your time (${pad(u.h)}:${pad(u.m)} UTC).`;
    }
    case "custom":
      return "Custom schedule (cron, UTC).";
  }
}

/** Minimal 5-field validity check for the custom cron field. */
export function isValidCron(cron: string): boolean {
  return cron.trim().split(/\s+/).length === 5;
}

function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

export const WEEKDAYS = DAYS;
