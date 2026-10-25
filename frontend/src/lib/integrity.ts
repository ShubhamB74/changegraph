// "What this report cannot tell you." The UI always shows these; analysis gaps are never hidden.

import { plural } from "./format.ts";
import type { ReportModel } from "./model.ts";

export type NoteLevel = "limit" | "caution" | "info";

export interface IntegrityNote {
  id: string;
  level: NoteLevel;
  text: string;
}

export const STANDING_CAVEAT =
  "Based on static imports only. A dependency or recommendation here does not prove that a test will catch a regression.";

export function integrityNotes(model: ReportModel): IntegrityNote[] {
  const r = model.report;
  const notes: IntegrityNote[] = [];
  const s = r.graph.stats;

  if (r.test_recommendations.completeness.complete === false) {
    for (const reason of r.test_recommendations.completeness.reasons) {
      notes.push({ id: `tests-${reason.kind}`, level: "limit", text: `Test selection is incomplete: ${reason.detail}.` });
    }
  }
  if (r.impact.depth_limited) {
    notes.push({
      id: "depth",
      level: "limit",
      text: `Impact traversal stopped at depth ${r.impact.max_depth ?? "?"}; more dependents exist beyond it.`,
    });
  }
  if (r.warnings.some((w) => w.code === "graph_target_mismatch")) {
    notes.push({
      id: "mismatch",
      level: "limit",
      text: "The dependency graph was built from the checked-out files, not from the comparison target revision.",
    });
  }
  if (s.parsed_partial + s.parsed_failed > 0) {
    notes.push({
      id: "parse",
      level: "caution",
      text: `${plural(s.parsed_partial + s.parsed_failed, "file")} could not be fully parsed; imports in them may be missing.`,
    });
  }
  if (s.unresolved_imports > 0) {
    notes.push({
      id: "unresolved",
      level: "caution",
      text: `${plural(s.unresolved_imports, "import")} could not be resolved and are not drawn as dependencies.`,
    });
  }
  if (r.impact.uncertain.length > 0) {
    notes.push({
      id: "uncertain",
      level: "caution",
      text: `${plural(r.impact.uncertain.length, "file")} may depend on changed code through imports the graph cannot see.`,
    });
  }
  const unanalyzed = r.impact.ignored_changed_files.filter((f) => f.reason !== "non_source_file").length;
  if (unanalyzed > 0) {
    notes.push({
      id: "ignored",
      level: "caution",
      text: `${plural(unanalyzed, "changed source file")} ${unanalyzed === 1 ? "was" : "were"} excluded or missing from the graph.`,
    });
  }
  if (r.warnings.some((w) => w.code === "dirty_working_tree")) {
    notes.push({ id: "dirty", level: "caution", text: "Uncommitted changes exist but are not part of this comparison." });
  }
  if (r.warnings.some((w) => w.code === "unmodeled_config_change")) {
    notes.push({ id: "config", level: "caution", text: "A configuration or dependency file changed; its effect is not modelled." });
  }
  if (r.impact.summary.changed_files === 0 && r.impact.ignored_changed_files.length === 0) {
    notes.push({ id: "nochange", level: "info", text: "No changed files were found, so there is no impact to show." });
  }
  return notes;
}
