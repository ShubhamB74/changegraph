import type { ImpactState } from "./model.ts";

/** Colour carries the impact state; border style repeats it so colour is never the only cue. */
export const STATE_STYLE: Record<ImpactState, { color: string; border: "solid" | "dashed" | "dotted"; glyph: string }> = {
  changed: { color: "#F5B942", border: "solid", glyph: "●" },
  removed: { color: "#F5B942", border: "dashed", glyph: "✕" },
  direct: { color: "#F2668B", border: "solid", glyph: "◆" },
  transitive: { color: "#A08BFF", border: "solid", glyph: "◇" },
  uncertain: { color: "#C5CEDC", border: "dashed", glyph: "?" },
  unaffected: { color: "#46556B", border: "solid", glyph: "·" },
};

export const TEST_COLOR = "#4FD1C5";
