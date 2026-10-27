#!/usr/bin/env node
// Verifies that every relative Markdown link points at an existing file, and that every page in
// docs/ is reachable from docs/index.md. Exits 1 and lists the problems otherwise.
// Usage: node scripts/check-doc-links.mjs [repo-root]

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";

const root = resolve(process.argv[2] ?? ".");
const SKIP = new Set(["node_modules", "target", ".git", ".next", "out"]);

function markdownFiles(dir) {
  const found = [];
  for (const name of readdirSync(dir)) {
    if (SKIP.has(name)) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) found.push(...markdownFiles(p));
    else if (name.endsWith(".md")) found.push(p);
  }
  return found;
}

/** Links in `text`, ignoring fenced code blocks and inline code. */
export function linksIn(text) {
  const withoutFences = text.replace(/^```[\s\S]*?^```/gm, "").replace(/`[^`\n]*`/g, "");
  const links = [];
  for (const m of withoutFences.matchAll(/\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) links.push(m[1]);
  return links;
}

export function isLocal(target) {
  return !/^([a-z][a-z0-9+.-]*:|#|\/\/)/i.test(target);
}

function main() {
  const problems = [];
  const files = markdownFiles(root);
  const linkedFromIndex = new Set();
  const indexPath = join(root, "docs", "index.md");

  for (const file of files) {
    for (const target of linksIn(readFileSync(file, "utf8"))) {
      if (!isLocal(target)) continue;
      const pathPart = decodeURIComponent(target.split("#")[0]);
      if (!pathPart) continue;
      const resolved = pathPart.startsWith("/") ? join(root, pathPart) : resolve(dirname(file), pathPart);
      if (!existsSync(resolved)) {
        problems.push(`${relative(root, file)}: broken link -> ${target}`);
      } else if (file === indexPath) {
        linkedFromIndex.add(resolved);
      }
    }
  }

  if (existsSync(indexPath)) {
    for (const file of files) {
      const rel = relative(join(root, "docs"), file);
      if (rel.startsWith("..") || file === indexPath || rel.includes("/")) continue; // top-level docs pages only
      if (!linkedFromIndex.has(file)) problems.push(`docs/${rel}: not linked from docs/index.md`);
    }
  } else {
    problems.push("docs/index.md is missing");
  }

  if (problems.length > 0) {
    console.error(problems.join("\n"));
    console.error(`\n${problems.length} documentation problem(s) in ${files.length} Markdown file(s).`);
    process.exit(1);
  }
  console.log(`documentation links ok (${files.length} Markdown files)`);
}

import { fileURLToPath } from "node:url";
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
