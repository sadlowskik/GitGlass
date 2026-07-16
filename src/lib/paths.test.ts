import { describe, expect, it } from "vitest";
import { baseName, isWindowsPath, segments } from "./paths";

describe("baseName", () => {
  it("returns the last segment of a windows path", () => {
    expect(baseName("C:\\Users\\me\\project")).toBe("project");
  });
  it("returns the last segment of a posix path", () => {
    expect(baseName("/home/me/project")).toBe("project");
  });
  it("handles trailing separators", () => {
    expect(baseName("C:\\Users\\me\\project\\")).toBe("project");
  });
});

describe("isWindowsPath", () => {
  it("detects drive-letter paths", () => {
    expect(isWindowsPath("C:\\x")).toBe(true);
  });
  it("treats posix paths as non-windows", () => {
    expect(isWindowsPath("/home/me")).toBe(false);
  });
});

describe("segments", () => {
  it("builds cumulative windows paths", () => {
    const segs = segments("C:\\Users\\me\\proj");
    expect(segs.map((s) => s.label)).toEqual(["C:", "Users", "me", "proj"]);
    expect(segs[0].path).toBe("C:\\");
    expect(segs[3].path).toBe("C:\\Users\\me\\proj");
  });

  it("builds cumulative posix paths", () => {
    const segs = segments("/home/me/proj");
    expect(segs.map((s) => s.label)).toEqual(["home", "me", "proj"]);
    expect(segs[0].path).toBe("/home");
    expect(segs[2].path).toBe("/home/me/proj");
  });
});
