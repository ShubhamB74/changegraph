import type { ReactNode } from "react";
import { splitPath } from "../lib/format.ts";
import { STATE_LABEL } from "../lib/model.ts";
import type { FileNodeInfo, ReportModel } from "../lib/model.ts";
import type { TestMethod, UncertaintyReason } from "../lib/report-types.ts";
import { STATE_STYLE } from "../lib/theme.ts";

const METHOD_LABEL: Record<TestMethod, string> = {
  changed_test: "test file changed",
  direct_import: "direct import",
  dependency_reachability: "reaches through imports",
  coverage_map: "coverage map",
  filename_heuristic: "filename guess",
};

const REASON_LABEL: Record<UncertaintyReason["kind"], string> = {
  unresolved_import: "has an import the tool could not resolve",
  parse_partial: "has syntax errors, so some imports may be missing",
  parse_failed: "could not be parsed",
};

interface Props {
  model: ReportModel;
  path: string | null;
  showPath: boolean;
  onShowPath: (v: boolean) => void;
  onSelect: (path: string) => void;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-ink-line px-4 py-3">
      <h3 className="mb-1.5 text-sm font-semibold">{title}</h3>
      {children}
    </section>
  );
}

function PathButton({ path, onSelect }: { path: string; onSelect: (p: string) => void }) {
  return (
    <button
      type="button"
      onClick={() => onSelect(path)}
      className="break-all text-left font-mono text-xs text-ink-text underline decoration-ink-faint underline-offset-2 hover:decoration-ink-text"
    >
      {path}
    </button>
  );
}

function PathList({ paths, onSelect, empty }: { paths: string[]; onSelect: (p: string) => void; empty: string }) {
  if (paths.length === 0) return <p className="text-sm text-ink-dim">{empty}</p>;
  return (
    <ul className="flex flex-col gap-1">
      {paths.slice(0, 50).map((p) => (
        <li key={p}>
          <PathButton path={p} onSelect={onSelect} />
        </li>
      ))}
      {paths.length > 50 && <li className="text-xs text-ink-faint">and {paths.length - 50} more</li>}
    </ul>
  );
}

export function Inspector({ model, path, showPath, onShowPath, onSelect }: Props) {
  if (!path) {
    return (
      <div className="p-4 text-sm text-ink-dim">
        <h2 className="mb-1 text-sm font-semibold text-ink-text">Details</h2>
        <p>Select a file on the graph or in the lists to see why it is in this report, the import chain behind it, and which tests are linked to it.</p>
      </div>
    );
  }
  const node = model.nodes.get(path);
  if (!node) return <div className="p-4 text-sm text-ink-dim">This file is not in the report.</div>;
  return <Details model={model} node={node} showPath={showPath} onShowPath={onShowPath} onSelect={onSelect} />;
}

function Details({
  model,
  node,
  showPath,
  onShowPath,
  onSelect,
}: {
  model: ReportModel;
  node: FileNodeInfo;
  showPath: boolean;
  onShowPath: (v: boolean) => void;
  onSelect: (p: string) => void;
}) {
  const { dir, base } = splitPath(node.path);
  const style = STATE_STYLE[node.state];
  const warnings = model.report.warnings.filter((w) => w.message.includes(node.path));
  const a = node.affected;
  const isImpactedSource = node.category === "source" && (node.state === "changed" || node.state === "direct" || node.state === "transitive" || node.state === "removed");

  return (
    <article aria-label={`Details for ${node.path}`}>
      <header className="border-b border-ink-line px-4 py-3">
        <h2 className="break-all font-mono text-sm font-medium">
          <span className="text-ink-faint">{dir ? `${dir}/` : ""}</span>
          {base}
        </h2>
        <p className="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
          <span style={{ color: style.color }}>
            <span aria-hidden>{style.glyph} </span>
            {STATE_LABEL[node.state]}
          </span>
          <span className="text-ink-dim">{node.category === "test" ? "test file" : "source file"}</span>
          {node.parseStatus && node.parseStatus !== "ok" && (
            <span className="text-signal-limit">
              {node.parseStatus === "partial" ? "partially parsed" : "not parsed"}
            </span>
          )}
        </p>
      </header>

      {node.changeStatus && (
        <Section title="Change">
          <p className="text-sm">
            {node.changeStatus.replace("_", " ")}
            {node.changed && (
              <span className="text-ink-dim">
                {" "}
                &middot; {node.changed.direct_dependents} direct and {node.changed.transitive_dependents} transitive dependents
              </span>
            )}
          </p>
          {node.ghost && (
            <p className="mt-1 text-sm text-ink-dim">
              This file no longer exists. Files that still import it are shown as affected, with a dashed edge.
            </p>
          )}
        </Section>
      )}

      {a && (
        <Section title="Why it is affected">
          <p className="text-sm">{a.explanation}</p>
          <p className="mt-1 text-xs text-ink-dim">
            Rank {a.rank} &middot; distance {a.distance} &middot; reaches {a.changed_dependency_count} changed{" "}
            {a.changed_dependency_count === 1 ? "file" : "files"}
          </p>
          {a.affected_by.length > 1 && (
            <ul className="mt-1 text-xs text-ink-dim">
              {a.affected_by.map((b) => (
                <li key={b.changed_file}>
                  <span className="font-mono">{b.changed_file}</span> at distance {b.distance}
                </li>
              ))}
            </ul>
          )}
        </Section>
      )}

      {a && (
        <Section title="Import chain from the change">
          <label className="mb-2 flex cursor-pointer items-center gap-2 text-sm">
            <input type="checkbox" checked={showPath} onChange={(e) => onShowPath(e.target.checked)} />
            Highlight this chain on the graph
          </label>
          <ol className="flex flex-col gap-2 text-sm">
            {a.path_from_changed.map((p, i) => {
              const hop = i > 0 ? a.hops[i - 1] : undefined;
              return (
                <li key={`${i}-${p}`}>
                  <PathButton path={p} onSelect={onSelect} />
                  {i === 0 && <span className="ml-2 text-xs text-signal-changed">changed</span>}
                  {hop && (
                    <p className="mt-0.5 text-xs text-ink-dim">
                      imports <span className="font-mono">{hop.imported}</span> as{" "}
                      <span className="font-mono">&quot;{hop.specifier}&quot;</span>, line {hop.line}
                      {hop.type_only ? ", type-only" : ""}
                    </p>
                  )}
                </li>
              );
            })}
          </ol>
        </Section>
      )}

      {node.uncertain && (
        <Section title="Why it is uncertain">
          <p className="mb-1 text-sm text-ink-dim">
            The graph does not connect this file to the change, but it might depend on it in a way the tool cannot see.
          </p>
          <ul className="text-sm">
            {node.uncertain.reasons.map((r, i) => (
              <li key={i}>
                This file {REASON_LABEL[r.kind]}
                {r.specifier && (
                  <>
                    {": "}
                    <span className="font-mono">&quot;{r.specifier}&quot;</span>
                    {r.line ? `, line ${r.line}` : ""}
                  </>
                )}
                .
              </li>
            ))}
          </ul>
        </Section>
      )}

      <Section title="Tests">
        {node.recommendation && (
          <div className="mb-2 text-sm">
            <p>
              <span className="font-medium">Recommended test, rank {node.recommendation.rank}</span>{" "}
              <span className={node.recommendation.basis === "known" ? "text-ink-dim" : "text-signal-limit"}>
                ({node.recommendation.basis === "known" ? "known relationship" : "heuristic guess"})
              </span>
            </p>
            <p className="mt-1">{node.recommendation.explanation}</p>
            <ul className="mt-1 flex flex-col gap-1 text-xs text-ink-dim">
              {node.recommendation.covers.map((c, i) => (
                <li key={`${c.module}-${i}`}>
                  <span className="font-mono">{c.module}</span> ({c.module_status}): {METHOD_LABEL[c.method]}
                  {c.distance ? `, ${c.distance} ${c.distance === 1 ? "import" : "imports"} away` : ""}
                  {c.via ? ` via ${c.via}` : ""}
                </li>
              ))}
            </ul>
          </div>
        )}
        {node.category === "source" && node.coveredBy.length > 0 && (
          <ul className="flex flex-col gap-1.5 text-sm">
            {node.coveredBy.map((t) => (
              <li key={t.path}>
                <PathButton path={t.path} onSelect={onSelect} />
                <span className={`ml-2 text-xs ${t.basis === "known" ? "text-ink-dim" : "text-signal-limit"}`}>
                  {t.basis === "known" ? "known" : "guess"}
                </span>
              </li>
            ))}
          </ul>
        )}
        {isImpactedSource && node.coveredBy.length === 0 && (
          <p className="text-sm text-signal-limit">
            No test is linked to this file by a known method. Its tests, if any, may be missing from the recommendations.
          </p>
        )}
        {!node.recommendation && node.coveredBy.length === 0 && !isImpactedSource && (
          <p className="text-sm text-ink-dim">No recommended test relates to this file.</p>
        )}
      </Section>

      {(node.unresolved.length > 0 || node.external.length > 0 || warnings.length > 0) && (
        <Section title="Imports and warnings">
          {warnings.map((w, i) => (
            <p key={i} className="mb-1 text-sm text-signal-limit">
              {w.message}
            </p>
          ))}
          {node.unresolved.length > 0 && (
            <>
              <p className="text-xs text-ink-dim">Could not be resolved (not drawn as dependencies):</p>
              <ul className="mb-2 text-sm">
                {node.unresolved.map((u, i) => (
                  <li key={i}>
                    <span className="font-mono">&quot;{u.specifier}&quot;</span>, line {u.line}{" "}
                    <span className="text-xs text-ink-dim">({u.reason.replace(/_/g, " ")})</span>
                  </li>
                ))}
              </ul>
            </>
          )}
          {node.external.length > 0 && (
            <>
              <p className="text-xs text-ink-dim">External packages:</p>
              <p className="text-sm">{[...new Set(node.external.map((x) => x.package))].join(", ")}</p>
            </>
          )}
        </Section>
      )}

      <Section title="Direct relationships">
        <p className="mb-1 text-xs text-ink-dim">Imports ({node.dependsOn.length})</p>
        <PathList paths={node.dependsOn} onSelect={onSelect} empty="Imports no other analyzed file." />
        <p className="mb-1 mt-3 text-xs text-ink-dim">Imported by ({node.dependedOnBy.length})</p>
        <PathList paths={node.dependedOnBy} onSelect={onSelect} empty="No analyzed file imports this one." />
      </Section>
    </article>
  );
}
