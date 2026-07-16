// Cross-platform path helpers used by the UI. The backend hands us native
// absolute paths (Windows `C:\...` or POSIX `/...`); these parse both so the
// app behaves identically on Windows now and macOS later.

export function isWindowsPath(path: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(path) || path.includes("\\");
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Split an absolute path into clickable breadcrumb segments. */
export function segments(path: string): { label: string; path: string }[] {
  const win = isWindowsPath(path);
  const sep = win ? "\\" : "/";
  const parts = path.split(/[\\/]+/).filter(Boolean);
  const out: { label: string; path: string }[] = [];
  let acc = "";
  parts.forEach((part, i) => {
    acc = i === 0 ? (win ? part : sep + part) : acc + sep + part;
    out.push({ label: part, path: win && i === 0 ? part + sep : acc });
  });
  return out;
}
