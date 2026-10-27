#!/usr/bin/env node
// Builds a `changegraph-eval historical` dataset by ACTUALLY RUNNING a project's tests.
//
//   node collect-outcomes.mjs --repo /path/to/repo --pairs pairs.txt --out dataset.json \
//        --setup "npm ci" --cmd "npx jest --json --outputFile={out}" [--runs 2]
//
// pairs.txt: one "<base-sha> <head-sha>" per line ("#" starts a comment).
// For each pair the script checks out `base` and `head` in throwaway worktrees, runs --setup then
// --cmd (which must write Jest/Vitest-style JSON to {out}), and records
//
//     failing_after_change = (test files failing in EVERY head run) minus (test files failing in ANY base run)
//
// Running several times and requiring consistency is a guard against flaky tests, not a cure.
// The result is only as trustworthy as the project's test suite and environment.

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Failed test files from a Jest/Vitest `--json` report, as repo-relative POSIX paths. */
export function failedTestFiles(report, root) {
  const out = new Set();
  const results = Array.isArray(report?.testResults) ? report.testResults : [];
  for (const r of results) {
    const assertions = Array.isArray(r.assertionResults) ? r.assertionResults : [];
    // A suite that fails to even load has status "failed" and no assertions: still a failure.
    const failed = r.status === "failed" || assertions.some((a) => a.status === "failed");
    if (!failed) continue;
    const name = r.name ?? r.testFilePath;
    if (typeof name !== "string") continue;
    const rel = isAbsolute(name) ? relative(root, name) : name;
    out.add(rel.replace(/\\/g, "/"));
  }
  return out;
}

/** Tests failing in every head run but in no base run. Sorted. */
export function newlyFailing(headRuns, baseRuns) {
  if (headRuns.length === 0) return [];
  const consistentHead = [...headRuns[0]].filter((t) => headRuns.every((run) => run.has(t)));
  const anyBase = new Set(baseRuns.flatMap((run) => [...run]));
  return consistentHead.filter((t) => !anyBase.has(t)).sort();
}

function sh(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", ...opts });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(" ")} failed: ${r.stderr || r.error || r.status}`);
  return r.stdout.trim();
}

function withWorktree(repo, commit, fn) {
  const dir = mkdtempSync(join(tmpdir(), "cg-eval-"));
  const wt = join(dir, "wt");
  sh("git", ["-C", repo, "worktree", "add", "--detach", wt, commit]);
  try {
    return fn(wt);
  } finally {
    spawnSync("git", ["-C", repo, "worktree", "remove", "--force", wt]);
    rmSync(dir, { recursive: true, force: true });
  }
}

function collectRuns(wt, { setup, cmd, runs }) {
  if (setup) {
    const r = spawnSync(setup, { shell: true, cwd: wt, stdio: "inherit" });
    if (r.status !== 0) throw new Error(`setup command failed (${r.status}) in ${wt}`);
  }
  const sets = [];
  for (let i = 0; i < runs; i++) {
    const out = join(wt, `.cg-outcome-${i}.json`);
    // A non-zero exit is normal when tests fail; what matters is that the JSON report exists.
    spawnSync(cmd.replaceAll("{out}", out), { shell: true, cwd: wt, stdio: "inherit" });
    if (!existsSync(out)) {
      throw new Error(`the test command did not write its JSON report to {out}=${out}; check --cmd`);
    }
    sets.push(failedTestFiles(JSON.parse(readFileSync(out, "utf8")), wt));
  }
  return sets;
}

function parseArgs(argv) {
  const args = { runs: 2 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) throw new Error(`unexpected argument ${a}`);
    const key = a.slice(2);
    const val = argv[++i];
    if (val === undefined) throw new Error(`--${key} needs a value`);
    args[key] = key === "runs" ? Number(val) : val;
  }
  for (const k of ["repo", "pairs", "out", "cmd"]) if (!args[k]) throw new Error(`--${k} is required`);
  if (!Number.isInteger(args.runs) || args.runs < 1) throw new Error("--runs must be a positive integer");
  return args;
}

export function parsePairs(text) {
  return text
    .split("\n")
    .map((l) => l.replace(/#.*/, "").trim())
    .filter(Boolean)
    .map((l) => {
      const [base, head, ...rest] = l.split(/\s+/);
      if (!base || !head || rest.length) throw new Error(`bad pairs line: "${l}" (expected "<base> <head>")`);
      return { base, head };
    });
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const repo = resolve(args.repo);
  const pairs = parsePairs(readFileSync(args.pairs, "utf8"));
  const cases = [];
  for (const { base, head } of pairs) {
    console.error(`collecting ${base.slice(0, 8)}..${head.slice(0, 8)}`);
    const baseRuns = withWorktree(repo, base, (wt) => collectRuns(wt, args));
    const headRuns = withWorktree(repo, head, (wt) => collectRuns(wt, args));
    cases.push({
      id: head.slice(0, 12),
      base: sh("git", ["-C", repo, "rev-parse", base]),
      head: sh("git", ["-C", repo, "rev-parse", head]),
      failing_after_change: newlyFailing(headRuns, baseRuns),
      notes: `${args.runs} run(s) per commit; base failures: ${[...new Set(baseRuns.flatMap((s) => [...s]))].length}`,
    });
  }
  const dataset = {
    version: 1,
    repo,
    description: `Collected with: ${args.cmd} (${args.runs} run(s) per commit)`,
    cases,
  };
  writeFileSync(args.out, JSON.stringify(dataset, null, 2) + "\n");
  console.error(`wrote ${args.out} with ${cases.length} case(s)`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
