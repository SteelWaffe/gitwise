import { t } from "./i18n";
import type { ChangeKind } from "./types";

export const PROFILE_COLORS = ["#3b74f2", "#ff8a3d", "#22a06b", "#a855f7", "#e5484d", "#0ea5b7", "#d4a20b", "#64748b"];

export function relativeTime(unixSeconds: number): string {
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return t("time.justNow");
  if (diff < 3600) return t("time.minutes", { n: Math.floor(diff / 60) });
  if (diff < 86400) return t("time.hours", { n: Math.floor(diff / 3600) });
  if (diff < 86400 * 30) return t("time.days", { n: Math.floor(diff / 86400) });
  return new Date(unixSeconds * 1000).toLocaleDateString();
}

export function splitPath(p: string): { dir: string; name: string } {
  const i = p.lastIndexOf("/");
  return i < 0 ? { dir: "", name: p } : { dir: p.slice(0, i), name: p.slice(i + 1) };
}

export function initials(label: string): string {
  const parts = label.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}

export const KIND_LETTER: Record<ChangeKind, string> = {
  added: "A",
  untracked: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  conflicted: "!",
};

export function shortenHome(path: string): string {
  return path.replace(/^C:\\Users\\[^\\]+/i, "~").replace(/^\/Users\/[^/]+/, "~");
}

/** Whether a repository path matches one of the LFS patterns from .gitattributes (simple globs). */
export function matchesLfs(path: string, patterns: string[]): boolean {
  const name = splitPath(path).name;
  return patterns.some((p) => {
    const re = new RegExp(
      "^" +
        p
          .replace(/[.+^${}()|\\]/g, "\\$&")
          .replace(/\*\*/g, "§")
          .replace(/\*/g, "[^/]*")
          .replace(/§/g, ".*")
          .replace(/\?/g, ".") +
        "$",
      "i",
    );
    return p.includes("/") ? re.test(path) : re.test(name);
  });
}

export function joinPath(dir: string, name: string): string {
  if (!dir) return name;
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.replace(/[\\/]+$/, "") + sep + name;
}

export function repoNameFromUrl(url: string): string {
  const cleaned = url.trim().replace(/\/+$/, "").replace(/\.git$/, "");
  const parts = cleaned.split(/[/:]/);
  return parts[parts.length - 1] || "";
}
