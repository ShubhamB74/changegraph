import { percent } from "../lib/format.ts";
import type { ReportModel } from "../lib/model.ts";

function Row({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1">
      <dt className="text-ink-dim">{label}</dt>
      <dd className="text-right tabular-nums">
        <span className="font-medium">{value}</span>
        {hint && <span className="ml-2 text-xs text-ink-faint">{hint}</span>}
      </dd>
    </div>
  );
}

export function SummaryPanel({ model }: { model: ReportModel }) {
  const r = model.report;
  const s = r.graph.stats;
  const im = r.impact.summary;
  const t = r.test_recommendations.summary;
  const pct = percent(s.parsed_ok, s.files);
  const pctText = pct === 100 ? "100%" : `${pct.toFixed(1)}%`;
  return (
    <section aria-labelledby="summary-h" className="border-b border-ink-line px-4 py-3">
      <h2 id="summary-h" className="mb-1 text-sm font-semibold">
        Summary
      </h2>
      <dl className="text-sm">
        <Row label="Changed files" value={String(im.changed_files)} />
        <Row label="Affected files" value={String(im.affected)} hint={`${im.direct} direct, ${im.transitive} transitive`} />
        <Row
          label="Recommended tests"
          value={String(t.recommended)}
          hint={`of ${t.total_test_files}${t.heuristic_only > 0 ? `, ${t.heuristic_only} guessed` : ""}`}
        />
        <Row label="Parsed cleanly" value={pctText} hint={`${s.parsed_ok} of ${s.files}`} />
        <Row label="Unresolved imports" value={String(s.unresolved_imports)} />
        <Row label="Uncertain files" value={String(im.uncertain)} />
        <Row label="Import cycles" value={String(s.cycle_count)} />
      </dl>
    </section>
  );
}
