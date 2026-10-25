import { ALL_STATES } from "../lib/filters.ts";
import { STATE_LABEL } from "../lib/model.ts";
import { STATE_STYLE, TEST_COLOR } from "../lib/theme.ts";

export function Legend() {
  return (
    <details className="pointer-events-auto rounded border border-ink-line bg-ink-panel/95 text-xs" open>
      <summary className="cursor-pointer px-3 py-1.5 font-medium">Legend</summary>
      <ul className="flex flex-col gap-1 px-3 pb-2">
        {ALL_STATES.map((s) => (
          <li key={s} className="flex items-center gap-2">
            <span
              aria-hidden
              className="inline-block h-3 w-5 rounded-sm"
              style={{ border: `2px ${STATE_STYLE[s].border} ${STATE_STYLE[s].color}` }}
            />
            {STATE_LABEL[s]}
          </li>
        ))}
        <li className="flex items-center gap-2">
          <span aria-hidden style={{ color: TEST_COLOR }}>
            ▶
          </span>
          Recommended test
        </li>
        <li className="mt-1 max-w-[16rem] text-ink-dim">
          An arrow points from the importing file to the file it imports. Impact flows the other way, left to right.
          Dashed edges are imports of files that were removed.
        </li>
      </ul>
    </details>
  );
}
