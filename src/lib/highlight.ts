import hljs from "highlight.js/lib/common";

// Map file extensions to highlight.js language names. Unknown extensions fall
// back to escaped plain text (no highlighting), which is safe and readable.
const EXT_TO_LANG: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  rs: "rust",
  py: "python",
  rb: "ruby",
  go: "go",
  java: "java",
  kt: "kotlin",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  hpp: "cpp",
  cs: "csharp",
  php: "php",
  swift: "swift",
  css: "css",
  scss: "scss",
  less: "less",
  html: "xml",
  xml: "xml",
  vue: "xml",
  svelte: "xml",
  md: "markdown",
  markdown: "markdown",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  yml: "yaml",
  yaml: "yaml",
  toml: "ini",
  ini: "ini",
  sql: "sql",
  dockerfile: "dockerfile",
};

export function langForPath(path: string): string | undefined {
  const name = path.split("/").pop() ?? path;
  if (name.toLowerCase() === "dockerfile") return "dockerfile";
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  return EXT_TO_LANG[ext];
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

/**
 * Highlight a single line of code, returning safe HTML. Highlighting per-line
 * loses multi-line context (e.g. block comments) but is simple and good enough
 * for a diff view; highlight.js escapes all text so the output is safe.
 */
export function highlightLine(code: string, lang: string | undefined): string {
  if (code.length === 0) return "";
  if (lang) {
    try {
      if (hljs.getLanguage(lang)) {
        return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
      }
    } catch {
      // fall through to plain escaped text
    }
  }
  return escapeHtml(code);
}
