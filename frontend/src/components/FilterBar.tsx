import type { ChangeEvent } from "react";
import { ALL_STATES } from "../lib/filters.ts";
import type { Filters, Scope } from "../lib/filters.ts";
import { STATE_LABEL } from "../lib/model.ts";
import type { ImpactState, ReportModel } from "../lib/model.ts";
import { STATE_STYLE } from "../lib/theme.ts";

interface Props {
  model: ReportModel;
  filters: Filters;
  matchCount: number;
  onChange: (next: Filters) => void;
}

export function FilterBar({ model, filters, matchCount, onChange }: Props) {
  const counts: Record<ImpactState, number> = {
    changed: 0,
    removed: 0,
    direct: 0,
    transitive: 0,
    uncertain: 0,
    unaffected: 0,
  };
  for (const n of model.nodes.values()) counts[n.state]++;

  const setScope = (scope: Scope) => onChange({ ...filters, scope });
  const toggleState = (s: ImpactState) =>
    onChange({ ...filters, states: { ...filters.states, [s]: !filters.states[s] } });

  return (
    <section aria-labelledby="filters-h" className="border-b border-ink-line px-4 py-3">
      <h2 id="filters-h" className="mb-2 text-sm font-semibold">
        Find and filter
      </h2>

      <label className="sr-only" htmlFor="file-search">
        Search files by path
      </label>
      <input
        id="file-search"
        type="search"
        value={filters.query}
        placeholder="Search by path"
        onChange={(e: ChangeEvent<HTMLInputElement>) => onChange({ ...filters, query: e.target.value })}
        className="w-full rounded border border-ink-line bg-ink px-2 py-1.5 font-mono text-sm placeholder:text-ink-faint"
      />
      {filters.query.trim() !== "" && (
        <p className="mt-1 text-xs text-ink-dim" aria-live="polite">
          {matchCount} {matchCount === 1 ? "file matches" : "files match"}; matches outside the current scope are included.
        </p>
      )}

      <fieldset className="mt-3">
        <legend className="mb-1 text-xs text-ink-dim">Show</legend>
        <div className="flex overflow-hidden rounded border border-ink-line text-sm" role="radiogroup">
          {(
            [
              ["impact", "Impact only"],
              ["all", "All files"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={filters.scope === value}
              onClick={() => setScope(value)}
              className={`flex-1 px-2 py-1 ${filters.scope === value ? "bg-ink-raised font-medium" : "text-ink-dim hover:text-ink-text"}`}
            >
              {label}
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset className="mt-3">
        <legend className="mb-1 text-xs text-ink-dim">File states</legend>
        <ul className="grid grid-cols-1 gap-0.5 text-sm">
          {ALL_STATES.filter((s) => counts[s] > 0).map((s) => (
            <li key={s}>
              <label className="flex cursor-pointer items-center gap-2 py-0.5">
                <input type="checkbox" checked={filters.states[s]} onChange={() => toggleState(s)} />
                <span aria-hidden style={{ color: STATE_STYLE[s].color }}>
                  {STATE_STYLE[s].glyph}
                </span>
                <span className="flex-1">{STATE_LABEL[s]}</span>
                <span className="tabular-nums text-ink-faint">{counts[s]}</span>
              </label>
            </li>
          ))}
        </ul>
        <label className="mt-1 flex cursor-pointer items-center gap-2 py-0.5 text-sm">
          <input
            type="checkbox"
            checked={filters.showTests}
            onChange={() => onChange({ ...filters, showTests: !filters.showTests })}
          />
          <span>Show test files</span>
        </label>
      </fieldset>
    </section>
  );
}
