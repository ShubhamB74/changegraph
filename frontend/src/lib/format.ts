import type { Report } from "./report-types.ts";

export function splitPath(path: string): { dir: string; base: string } {
  const i = path.lastIndexOf("/");
  return i < 0 ? { dir: "", base: path } : { dir: path.slice(0, i), base: path.slice(i + 1) };
}

export function percent(part: number, whole: number): number {
  return whole === 0 ? 100 : (100 * part) / whole;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

export function shortSha(sha: string | null): string {
  return sha ? sha.slice(0, 7) : "";
}

/** One-line, plain-language description of what was compared. */
export function describeComparison(report: Report): string {
  const c = report.comparison;
  if (c.mode === "working_tree") {
    return `Uncommitted changes (staged, unstaged, untracked) vs HEAD ${shortSha(report.repository.head_commit)}`;
  }
  const base = c.base ?? "base";
  const target = c.target ?? "HEAD";
  return `Committed changes on ${target} since its merge base with ${base} (${shortSha(c.merge_base_commit)})`;
}
