import { describe, expect, it } from "vitest";
import { buildCron, isValidCron, type ScheduleInput } from "./cron";

const base: ScheduleInput = { frequency: "hourly", time: "09:00", weekday: 1, custom: "" };

describe("buildCron (timezone-independent frequencies)", () => {
  it("every 15 minutes", () => {
    expect(buildCron({ ...base, frequency: "every-15m" })).toBe("*/15 * * * *");
  });
  it("every 30 minutes", () => {
    expect(buildCron({ ...base, frequency: "every-30m" })).toBe("*/30 * * * *");
  });
  it("hourly", () => {
    expect(buildCron({ ...base, frequency: "hourly" })).toBe("0 * * * *");
  });
  it("every 6 hours", () => {
    expect(buildCron({ ...base, frequency: "every-6h" })).toBe("0 */6 * * *");
  });
  it("passes custom cron through", () => {
    expect(buildCron({ ...base, frequency: "custom", custom: "5 4 * * 0" })).toBe("5 4 * * 0");
  });
});

describe("isValidCron", () => {
  it("accepts a 5-field expression", () => {
    expect(isValidCron("*/15 * * * *")).toBe(true);
  });
  it("rejects the wrong number of fields", () => {
    expect(isValidCron("0 9 * *")).toBe(false);
    expect(isValidCron("0 9 * * * *")).toBe(false);
  });
});
