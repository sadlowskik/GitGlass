import { describe, it, expect } from "vitest";
import { isGithub } from "./useGithubStore";

describe("isGithub", () => {
  it("accepts the remote forms git actually writes", () => {
    for (const url of [
      "https://github.com/owner/repo.git",
      "https://github.com/owner/repo",
      "http://github.com/owner/repo.git",
      "git@github.com:owner/repo.git",
      "ssh://git@github.com/owner/repo.git",
      "https://GitHub.com/owner/repo.git",
      "  https://github.com/owner/repo.git  ",
    ]) {
      expect(isGithub(url), url).toBe(true);
    }
  });

  // Each of these contains the text "github.com" and so passed the old
  // `url.includes("github.com")` check while pointing somewhere else.
  it("rejects look-alike hosts", () => {
    for (const url of [
      "https://github.com@evil.example/owner/repo.git",
      "https://github.com.evil.example/owner/repo.git",
      "https://evil.example/github.com/owner/repo.git",
      "https://notgithub.com/owner/repo.git",
      "git@github.com.evil.example:owner/repo.git",
    ]) {
      expect(isGithub(url), url).toBe(false);
    }
  });

  it("handles missing and unparseable values", () => {
    expect(isGithub(null)).toBe(false);
    expect(isGithub(undefined)).toBe(false);
    expect(isGithub("")).toBe(false);
    expect(isGithub("not a url")).toBe(false);
  });
});
