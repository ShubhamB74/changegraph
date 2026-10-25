import { readFileSync } from "node:fs";
import type { Report } from "./report-types.ts";

export function sampleText(): string {
  return readFileSync(new URL("../../public/sample-report.json", import.meta.url), "utf8");
}

/** A fresh deep copy of the bundled sample report. */
export function sampleReport(): Report {
  return JSON.parse(sampleText()) as Report;
}

/** Small deterministic PRNG so "random" graphs are reproducible. */
export function lcg(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 0x100000000;
  };
}
